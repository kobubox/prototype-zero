use anyhow::Result;
use embedded_svc::{http::Method, io::Write as _};
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::nvs::EspDefaultNvs;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BlinkConfig {
    pub enabled: bool,
    pub period_ms: u64,
}

impl BlinkConfig {
    pub fn load(nvs: &EspDefaultNvs) -> Self {
        let enabled = nvs
            .get_u8("enabled")
            .ok()
            .flatten()
            .map(|b| b != 0)
            .unwrap_or(true);

        let period_ms = nvs
            .get_u32("period_ms")
            .ok()
            .flatten()
            .map(|v| v as u64)
            .unwrap_or(500);

        BlinkConfig { enabled, period_ms }
    }

    pub fn save(&self, nvs: &EspDefaultNvs) -> Result<()> {
        nvs.set_u8("enabled", if self.enabled { 1 } else { 0 })?;
        nvs.set_u32("period_ms", self.period_ms as u32)?;
        Ok(())
    }
}

/// Events emitted by the HTTP server
#[derive(Debug, Clone)]
pub enum ServerEvent {
    ConfigUpdated(BlinkConfig),
    DisplayText(String),
}

pub struct HttpServer {
    _server: EspHttpServer<'static>,
}

impl HttpServer {
    pub fn start<F>(config: BlinkConfig, nvs: EspDefaultNvs, on_event: F) -> Result<Self>
    where
        F: FnMut(ServerEvent) + Send + 'static,
    {
        let mut server = EspHttpServer::new(&HttpConfig::default())?;

        let blink_cfg = Arc::new(Mutex::new(config));
        let nvs_handle = Arc::new(Mutex::new(nvs));
        let event_callback = Arc::new(Mutex::new(on_event));

        // Root route: show form
        {
            let blink_cfg = blink_cfg.clone();
            server.fn_handler::<anyhow::Error, _>("/", Method::Get, move |req| {
                let mut resp = req.into_ok_response()?;

                let cfg = blink_cfg.lock().unwrap();
                let enabled_str = if cfg.enabled { "checked" } else { "" };
                let html = format!(
                    r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>ESP32 Blink Control</title>
  </head>
  <body>
    <h1>ESP32 Blink Control</h1>
    <p>Current period: {period_ms} ms, enabled: {enabled}</p>

    <h2>Blink Settings</h2>
    <form action="/set" method="GET">
      <label>
        Blink period (ms, full cycle):
        <input type="number" name="period" min="50" max="5000" value="{period_ms}">
      </label>
      <br><br>
      <label>
        <input type="checkbox" name="enabled" value="1" {enabled_checked}>
        Enable blinking
      </label>
      <br><br>
      <button type="submit">Apply</button>
    </form>

    <h2>E-Paper Display</h2>
    <form action="/display" method="GET">
      <label>
        Text to display:
        <input type="text" name="text" maxlength="100" placeholder="Enter text...">
      </label>
      <br><br>
      <button type="submit">Display</button>
    </form>
  </body>
</html>
"#,
                    period_ms = cfg.period_ms,
                    enabled = cfg.enabled,
                    enabled_checked = enabled_str,
                );

                resp.write_all(html.as_bytes())?;
                Ok(())
            })?;
        }

        // /set route: update config from query string, persist to NVS, emit event
        {
            let blink_cfg = blink_cfg.clone();
            let nvs_handle = nvs_handle.clone();
            let event_cb = event_callback.clone();

            server.fn_handler::<anyhow::Error, _>("/set", Method::Get, move |req| {
                let uri = req.uri();
                if let Some(qpos) = uri.find('?') {
                    let query = &uri[qpos + 1..];
                    let mut new_period = None;
                    let mut new_enabled = None;

                    for pair in query.split('&') {
                        let mut it = pair.splitn(2, '=');
                        let key = it.next().unwrap_or("");
                        let val = it.next().unwrap_or("");

                        match key {
                            "period" => {
                                if let Ok(p) = val.parse::<u64>() {
                                    new_period = Some(p.clamp(50, 5000));
                                }
                            }
                            "enabled" => {
                                new_enabled = Some(val == "1");
                            }
                            _ => {}
                        }
                    }

                    let updated_config = {
                        let mut cfg = blink_cfg.lock().unwrap();
                        if let Some(p) = new_period {
                            cfg.period_ms = p;
                        }
                        if let Some(e) = new_enabled {
                            cfg.enabled = e;
                        } else {
                            cfg.enabled = false;
                        }

                        log::info!("Updated blink config: {:?}", *cfg);

                        // Persist to NVS
                        if let Ok(nvs) = nvs_handle.lock() {
                            if let Err(e) = cfg.save(&*nvs) {
                                log::warn!("Failed to save config to NVS: {:?}", e);
                            } else {
                                log::info!("Config saved to NVS");
                            }
                        }

                        cfg.clone()
                    };

                    // Emit event
                    if let Ok(mut callback) = event_cb.lock() {
                        callback(ServerEvent::ConfigUpdated(updated_config));
                    }
                }

                // Redirect back to root
                let mut resp = req.into_response(302, Some("Found"), &[("Location", "/")])?;
                resp.write_all(b"Redirecting...\n")?;
                Ok(())
            })?;
        }

        // /display route: display text on e-paper
        {
            let event_cb = event_callback.clone();

            server.fn_handler::<anyhow::Error, _>("/display", Method::Get, move |req| {
                let uri = req.uri();
                if let Some(qpos) = uri.find('?') {
                    let query = &uri[qpos + 1..];
                    
                    for pair in query.split('&') {
                        let mut it = pair.splitn(2, '=');
                        let key = it.next().unwrap_or("");
                        let val = it.next().unwrap_or("");

                        if key == "text" {
                            // URL decode the text (simple implementation)
                            let text = val.replace("+", " ").replace("%20", " ");
                            
                            log::info!("Received display text request: {}", text);
                            
                            // Emit event
                            if let Ok(mut callback) = event_cb.lock() {
                                callback(ServerEvent::DisplayText(text));
                            }
                            break;
                        }
                    }
                }

                // Redirect back to root
                let mut resp = req.into_response(302, Some("Found"), &[("Location", "/")])?;
                resp.write_all(b"Redirecting...\n")?;
                Ok(())
            })?;
        }

        Ok(Self { _server: server })
    }
}
