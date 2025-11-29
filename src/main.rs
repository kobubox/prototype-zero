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

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::PinDriver,
    prelude::*,
    spi::{config::Config as SpiConfig, SpiDeviceDriver, SpiDriver, SpiDriverConfig},
};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

use log::info;

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");

mod epaper;
use epaper::{DisplayJob, DisplayManager};

#[derive(Clone, Debug)]
struct BlinkConfig {
    enabled: bool,
    // full cycle period in milliseconds (on + off)
    period_ms: u64,
}

fn load_blink_config(nvs: &EspDefaultNvs) -> BlinkConfig {
    // Defaults if nothing stored yet
    let default = BlinkConfig {
        enabled: true,
        period_ms: 500,
    };

    // Enabled: stored as u8 0 / 1
    let enabled = nvs
        .get_u8("enabled")
        .ok()
        .flatten()
        .map(|b| b != 0)
        .unwrap_or(default.enabled);

    // Period: stored as u32
    let period_ms = nvs
        .get_u32("period_ms")
        .ok()
        .flatten()
        .map(|v| v as u64)
        .unwrap_or(default.period_ms);

    BlinkConfig { enabled, period_ms }
}

fn save_blink_config(nvs: &EspDefaultNvs, cfg: &BlinkConfig) -> anyhow::Result<()> {
    // Safe to cast: we clamp to a few seconds anyway
    nvs.set_u8("enabled", if cfg.enabled { 1 } else { 0 })?;
    nvs.set_u32("period_ms", cfg.period_ms as u32)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    info!("=== Starting main() ===");

    // --- Peripherals & LED setup ---
    info!("Taking peripherals...");
    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    info!("Setting up LED on GPIO2...");
    let led_pin = pins.gpio2;

    let mut led = PinDriver::output(led_pin)?;
    led.set_low()?;
    info!("LED initialized");

    // --- E-Paper Display Setup ---
    info!("Setting up SPI for e-paper display...");
    // SPI pins: GPIO13 (MOSI/DIN), GPIO14 (SCLK)
    let spi_driver = SpiDriver::new(
        peripherals.spi2,
        pins.gpio14,                       // SCLK
        pins.gpio13,                       // MOSI (DIN)
        None::<esp_idf_hal::gpio::Gpio12>, // MISO not needed
        &SpiDriverConfig::default(),
    )?;
    info!("SPI driver created");

    let spi_config = SpiConfig::new()
        .baudrate(4.MHz().into())
        .data_mode(embedded_hal::spi::MODE_0);

    let spi = SpiDeviceDriver::new(
        spi_driver,
        Option::<esp_idf_hal::gpio::Gpio15>::None,
        &spi_config,
    )?;
    info!("SPI device driver created");

    // Control pins
    info!("Setting up control pins...");
    let cs = PinDriver::output(pins.gpio15)?;
    let dc = PinDriver::output(pins.gpio18)?; // Changed to GPIO18
    let rst = PinDriver::output(pins.gpio4)?;
    let busy = PinDriver::input(pins.gpio5)?;
    info!("Control pins configured");

    // Start the display manager
    info!("Starting display manager...");
    let display_manager = DisplayManager::start(spi, cs, dc, rst, busy)?;
    let display_handle = display_manager.handle();

    info!("E-Paper display initialized");

    // Show "Hello world" on the display
    info!("Submitting text job...");
    display_handle.submit(DisplayJob::ShowText("Hello world".to_string()))?;
    info!("Display text job submitted");

    // ---- NVS: open default partition + "blink" namespace ----
    let nvs_partition = EspDefaultNvsPartition::take()?; // default "nvs" partition
    let nvs_for_wifi = nvs_partition.clone(); // clone for Wi-Fi
    let nvs = EspDefaultNvs::new(nvs_partition, "blink", true)?; // namespace "blink"

    // Load persisted config (or defaults if first boot)
    let initial_cfg = load_blink_config(&nvs);
    info!("Initial blink config from NVS: {:?}", initial_cfg);

    // Shared blink config for threads
    let blink_cfg = Arc::new(Mutex::new(initial_cfg));

    // Shared NVS handle for HTTP handler to save changes
    let nvs_handle = Arc::new(Mutex::new(nvs));

    // --- Spawn blink thread ---
    {
        let blink_cfg = blink_cfg.clone();

        // Move LED driver into the blink thread
        std::thread::Builder::new()
            .stack_size(4096) // 4KB stack for blink thread
            .spawn(move || loop {
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
            })?;
    }

    // --- Wi-Fi setup ---
    let sys_loop = EspSystemEventLoop::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs_for_wifi))?,
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

    // /set route: update config from query string, persist to NVS, then redirect
    {
        let blink_cfg = blink_cfg.clone();
        let nvs_handle = nvs_handle.clone();
        server.fn_handler::<anyhow::Error, _>("/set", Method::Get, move |req| {
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
                    info!("Updated blink config (RAM): {:?}", *cfg);

                    // Persist to NVS
                    if let Ok(nvs) = nvs_handle.lock() {
                        if let Err(e) = save_blink_config(&*nvs, &cfg) {
                            log::warn!("Failed to save blink config to NVS: {:?}", e);
                        } else {
                            info!("Blink config saved to NVS");
                        }
                    }
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
