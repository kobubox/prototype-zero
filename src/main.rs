use core::convert::TryInto;
use std::{thread, time::Duration};

use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};

use esp_idf_hal::{
    gpio::PinDriver,
    prelude::*,
    spi::{config::Config as SpiConfig, SpiDeviceDriver, SpiDriver, SpiDriverConfig},
    uart::{config::Config as UartConfig, UartDriver},
};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

use log::info;

const SSID: &str = include_str!("../.wifi_ssid");
const PASSWORD: &str = include_str!("../.wifi_password");

mod barcode;
mod blinker;
mod epaper;
mod http_server;

use barcode::{BarcodeEvent, BarcodeScanner};
use blinker::Blinker;
use epaper::{DisplayJob, DisplayManager};
use http_server::{BlinkConfig, HttpServer, ServerEvent};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    info!("=== Starting main() ===");

    // --- Peripherals & LED setup ---
    info!("Taking peripherals...");
    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    info!("Setting up LED on GPIO2...");
    let led: PinDriver<'static, _, _> = PinDriver::output(pins.gpio2)?;
    info!("LED initialized");

    // ---- NVS: open default partition + "blink" namespace ----
    let nvs_partition = EspDefaultNvsPartition::take()?;
    let nvs_for_wifi = nvs_partition.clone();
    let nvs_partition_for_server = nvs_partition.clone();
    let nvs = EspDefaultNvs::new(nvs_partition, "blink", true)?;

    // Load initial blink configuration
    let initial_cfg = BlinkConfig::load(&nvs);
    info!("Initial blink config from NVS: {:?}", initial_cfg);

    // Start blinker with initial config
    info!("Starting blinker...");
    let blink_handle = Blinker::start(led, initial_cfg.enabled, initial_cfg.period_ms)?;
    info!("Blinker started");

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

    // Clear the display on startup with a full refresh
    info!("Clearing display...");
    display_handle.submit(DisplayJob::Clear)?;
    info!("Display cleared");

    // --- Barcode Scanner (GM65) Setup ---
    info!("Setting up barcode scanner...");

    // UART configuration for GM65: 9600 8N1
    let uart_config = UartConfig::new().baudrate(Hertz(9600));

    let uart = UartDriver::new(
        peripherals.uart1,
        pins.gpio17,                               // TX (ESP32 → GM65 RXD)
        pins.gpio16,                               // RX (GM65 TXD → ESP32)
        Option::<esp_idf_hal::gpio::Gpio9>::None,  // CTS (unused)
        Option::<esp_idf_hal::gpio::Gpio10>::None, // RTS (unused)
        &uart_config,
    )?;
    info!("UART driver created for barcode scanner");

    // Optional: Set up control pins for GM65
    // GPIO25 = TRIG, GPIO26 = LED, GPIO27 = BEEP
    // (For now we'll just start the scanner in its default manual mode)

    // Start barcode scanner worker
    let display_handle_for_barcode = display_handle.clone();
    let _barcode_scanner = BarcodeScanner::start(uart, move |event| match event {
        BarcodeEvent::Scanned(code) => {
            info!("Scanned barcode: {}", code);

            // Display the scanned code on line 0 of the e-paper
            if let Err(e) = display_handle_for_barcode.submit(DisplayJob::UpdateLine {
                line_number: 0,
                text: code,
            }) {
                log::error!("Failed to submit barcode display job: {:?}", e);
            }
        }
        BarcodeEvent::Error(err) => {
            log::warn!("Barcode scanner error: {}", err);
        }
    })?;

    info!("Barcode scanner started");

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

    // --- HTTP server with event-driven config updates ---
    let nvs_for_server = EspDefaultNvs::new(nvs_partition_for_server, "blink", true)?;

    let _server = HttpServer::start(initial_cfg, nvs_for_server, move |event| match event {
        ServerEvent::ConfigUpdated(config) => {
            info!("Received config update event: {:?}", config);
            if let Err(e) = blink_handle.update_config(config.enabled, config.period_ms) {
                log::error!("Failed to update blink config: {:?}", e);
            }
        }
        ServerEvent::DisplayText(text) => {
            info!("Received display text event: {}", text);
            if let Err(e) = display_handle.submit(DisplayJob::ShowText(text)) {
                log::error!("Failed to submit display job: {:?}", e);
            }
        }
        ServerEvent::UpdateLine { line_number, text } => {
            info!("Received update line {} event: {}", line_number, text);
            if let Err(e) = display_handle.submit(DisplayJob::UpdateLine { line_number, text }) {
                log::error!("Failed to submit update line job: {:?}", e);
            }
        }
    })?;

    info!("HTTP server started");

    // Keep objects alive
    core::mem::forget(wifi);
    core::mem::forget(display_manager);

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
