use core::convert::TryInto;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use embedded_svc::{
    http::Method,
    io::Write as _,
    wifi::{AuthMethod, ClientConfiguration, Configuration},
};

use esp_idf_hal::{delay::FreeRtos, gpio::PinDriver, prelude::*};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

use log::info;

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");

#[derive(Clone, Debug)]
struct BlinkConfig {
    enabled: bool,
    // full cycle period in milliseconds (on + off)
    period_ms: u64,
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    // --- Peripherals & LED setup ---
    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // Adjust GPIO here if your LED is on a different pin
    let led_pin = pins.gpio2;

    let mut led = PinDriver::output(led_pin)?;
    led.set_low()?;

    // Shared blink config (default: 500ms cycle, enabled)
    let blink_cfg = Arc::new(Mutex::new(BlinkConfig {
        enabled: true,
        period_ms: 500,
    }));

    // --- Spawn blink thread ---
    {
        let blink_cfg = blink_cfg.clone();

        // Move LED driver into the blink thread
        thread::spawn(move || loop {
            let (enabled, period) = {
                let cfg = blink_cfg.lock().unwrap();
                (cfg.enabled, cfg.period_ms)
            };

            if enabled {
                let half = (period / 2).max(10) as u32;

                let _ = led.set_high();
                FreeRtos::delay_ms(half);

                let _ = led.set_low();
                FreeRtos::delay_ms(half);
            } else {
                let _ = led.set_low();
                FreeRtos::delay_ms(100);
            }
        });
    }

    // --- Wi-Fi setup ---
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("WiFi up, DHCP info: {:?}", ip_info);
    info!("Open http://{} in your browser", ip_info.ip);

    // --- HTTP server with blink control UI ---
    let mut server = EspHttpServer::new(&HttpConfig::default())?;

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

    // /set route: update config from query string, then redirect
    {
        let blink_cfg = blink_cfg.clone();
        server.fn_handler::<anyhow::Error, _>("/set", Method::Get, move |mut req| {
            let uri = req.uri(); // e.g. "/set?period=250&enabled=1"
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
                            // presence of enabled=1 checkbox means "on"
                            new_enabled = Some(val == "1");
                        }
                        _ => {}
                    }
                }

                {
                    let mut cfg = blink_cfg.lock().unwrap();
                    if let Some(p) = new_period {
                        cfg.period_ms = p;
                    }
                    if let Some(e) = new_enabled {
                        cfg.enabled = e;
                    } else {
                        // If checkbox is absent in query, it means unchecked
                        cfg.enabled = false;
                    }
                    info!("Updated blink config: {:?}", *cfg);
                }
            }

            // Redirect back to root
            let mut resp = req.into_response(302, Some("Found"), &[("Location", "/")])?;
            resp.write_all(b"Redirecting...\n")?;
            Ok(())
        })?;
    }

    // Keep objects alive
    core::mem::forget(wifi);
    core::mem::forget(server);

    // Park main thread forever
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;
    wifi.start()?;
    info!("Wi-Fi driver started");

    wifi.connect()?;
    info!("Wi-Fi connecting…");

    wifi.wait_netif_up()?;
    info!("Wi-Fi netif up");

    Ok(())
}
