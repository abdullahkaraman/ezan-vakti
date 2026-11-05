pub mod diyanet;
pub mod schedule;
pub mod store;
pub mod tray_digits;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use diyanet::{Api, Item};
use schedule::{Boundary, Prayer};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;
use store::{CachedList, Location, Saved, Store};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, State, WebviewWindow, WindowEvent,
};

const REFRESH_BELOW_DAYS: i64 = 7;
const RETRY_AFTER_MINUTES: i64 = 60;
const LIST_MAX_AGE_DAYS: i64 = 30;
const EDGE_MARGIN: f64 = 8.0;
// The monochrome template image only works on the macOS menu bar; on a dark
// Windows taskbar it would be invisible.
#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/32x32.png");
/// The click that closes the popover also lands on the tray icon; without this
/// window the blur hides it and the click immediately reopens it.
const REOPEN_GUARD_MS: u128 = 300;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Row {
    name: &'static str,
    time: String,
    current: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct View {
    title: String,
    #[serde(skip)]
    tray_label: Option<String>,
    next: Option<&'static str>,
    remaining: Option<String>,
    location: Option<String>,
    timezone: Option<String>,
    timezone_guessed: bool,
    rows: Vec<Row>,
    error: Option<String>,
}

struct Live {
    saved: Saved,
    zone: Tz,
    boundaries: Vec<Boundary>,
    error: Option<String>,
    next_attempt: DateTime<Utc>,
    refreshing: bool,
}

fn mac_zone() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse().ok())
        .unwrap_or(Tz::UTC)
}

fn zone_of(saved: &Saved) -> Tz {
    saved
        .location
        .as_ref()
        .and_then(|location| location.timezone.as_deref())
        .and_then(|name| name.parse().ok())
        .unwrap_or_else(mac_zone)
}

impl Live {
    fn new(saved: Saved, now: DateTime<Utc>) -> Self {
        let zone = zone_of(&saved);
        let boundaries = schedule::boundaries(&saved.days, zone);
        Self {
            saved,
            zone,
            boundaries,
            error: None,
            next_attempt: now,
            refreshing: false,
        }
    }

    fn replace(&mut self, saved: Saved) {
        self.zone = zone_of(&saved);
        self.boundaries = schedule::boundaries(&saved.days, self.zone);
        self.saved = saved;
    }

    fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        self.saved.location.is_some()
            && !self.refreshing
            && now >= self.next_attempt
            && schedule::days_left(&self.saved.days, self.zone, now) < REFRESH_BELOW_DAYS
    }

    fn view(&self, now: DateTime<Utc>) -> View {
        let status = schedule::status(&self.boundaries, now);
        let location = self.saved.location.as_ref();
        let title = match (location, &status) {
            (None, _) => "Konum seç".to_string(),
            (Some(_), None) => "Vakit yok".to_string(),
            (Some(_), Some(status)) => schedule::title(status),
        };
        let current = status.as_ref().and_then(|status| status.current);
        let rows = schedule::today(&self.saved.days, self.zone, now)
            .map(|day| {
                Prayer::ALL
                    .into_iter()
                    .zip(day.times)
                    .map(|(prayer, time)| Row {
                        name: prayer.name(),
                        time: time.format("%H:%M").to_string(),
                        current: current == Some(prayer),
                    })
                    .collect()
            })
            .unwrap_or_default();
        View {
            title,
            tray_label: status
                .as_ref()
                .map(|status| schedule::tray_label(status.remaining_secs)),
            next: status.as_ref().map(|status| status.next.prayer.dative()),
            remaining: status
                .as_ref()
                .map(|status| schedule::format_remaining(status.remaining_secs)),
            location: location.map(|l| {
                if l.district == l.city {
                    l.city.clone()
                } else {
                    format!("{}, {}", l.district, l.city)
                }
            }),
            timezone: location.map(|_| self.zone.name().to_string()),
            timezone_guessed: location.is_some_and(|l| l.timezone.is_none()),
            rows,
            error: self.error.clone(),
        }
    }
}

struct Core {
    api: Api,
    store: Store,
    live: Mutex<Live>,
    hidden_at: Mutex<Option<Instant>>,
}

impl Core {
    fn view(&self) -> View {
        self.live.lock().unwrap().view(Utc::now())
    }

    async fn list(&self, key: &str, path: &str) -> Result<Vec<Item>, String> {
        let now = Utc::now();
        let cached = self.store.load_list(key);
        if let Some(list) = &cached {
            if now - list.fetched_at < Duration::days(LIST_MAX_AGE_DAYS) {
                return Ok(list.items.clone());
            }
        }
        match self.api.items(path).await {
            Ok(items) => {
                let list = CachedList {
                    fetched_at: now,
                    items: items.clone(),
                };
                self.store.save_list(key, &list);
                Ok(items)
            }
            // An old list beats no list; these names almost never change.
            Err(error) => cached.map(|list| list.items).ok_or(error.message()),
        }
    }
}

#[tauri::command]
fn get_view(core: State<'_, Core>) -> View {
    core.view()
}

#[tauri::command]
async fn list_countries(core: State<'_, Core>) -> Result<Vec<Item>, String> {
    let mut items = core.list("countries", "/ulkeler").await?;
    diyanet::turkiye_first(&mut items);
    Ok(items)
}

#[tauri::command]
async fn list_cities(core: State<'_, Core>, country_id: String) -> Result<Vec<Item>, String> {
    core.list(
        &format!("cities-{country_id}"),
        &format!("/sehirler/{country_id}"),
    )
    .await
}

#[tauri::command]
async fn list_districts(core: State<'_, Core>, city_id: String) -> Result<Vec<Item>, String> {
    core.list(
        &format!("districts-{city_id}"),
        &format!("/ilceler/{city_id}"),
    )
    .await
}

#[derive(Deserialize)]
struct Selection {
    country: Item,
    city: Item,
    district: Item,
}

/// Commits only after the fetch succeeds, so a failed change leaves the previous
/// location and its times in place.
#[tauri::command]
async fn set_location(
    app: AppHandle,
    core: State<'_, Core>,
    selection: Selection,
) -> Result<View, String> {
    let Selection {
        country,
        city,
        district,
    } = selection;
    let days = core
        .api
        .times(&district.id)
        .await
        .map_err(|error| error.message())?;
    let timezone = core
        .api
        .find_timezone(
            &country.id,
            &country.name_en,
            &city.name_en,
            &district.name_en,
        )
        .await;
    let saved = Saved {
        location: Some(Location {
            country_id: country.id,
            country_en: country.name_en,
            city: city.name,
            city_en: city.name_en,
            district_id: district.id,
            district: district.name,
            district_en: district.name_en,
            timezone,
        }),
        days,
    };
    core.store.save(&saved);
    let view = {
        let mut live = core.live.lock().unwrap();
        live.replace(saved);
        live.error = None;
        live.view(Utc::now())
    };
    publish(&app, &view);
    Ok(view)
}

fn publish(app: &AppHandle, view: &View) {
    if let Some(tray) = app.tray_by_id("main") {
        // `set_title` does nothing on Windows; the tooltip and icon stand in.
        let _ = tray.set_title(Some(&view.title));
        let _ = tray.set_tooltip(Some(&view.title));
        #[cfg(windows)]
        show_count_in_icon(&tray, view.tray_label.as_deref());
    }
    let _ = app.emit("view-changed", view);
}

#[cfg(windows)]
fn show_count_in_icon(tray: &TrayIcon, label: Option<&str>) {
    let icon = match label {
        Some(label) => Some(Image::new_owned(
            tray_digits::render(label),
            tray_digits::SIZE,
            tray_digits::SIZE,
        )),
        None => Image::from_bytes(TRAY_ICON).ok(),
    };
    let _ = tray.set_icon(icon);
}

/// Windows hides tray icons in an overflow menu, so opening the app has to put
/// something on screen itself: on first run and on a second launch.
#[cfg(windows)]
fn show_centered(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

async fn refresh(app: AppHandle, location: Location) {
    let core = app.state::<Core>();
    let result = core.api.times(&location.district_id).await;
    let timezone = match &location.timezone {
        Some(zone) => Some(zone.clone()),
        None => {
            core.api
                .find_timezone(
                    &location.country_id,
                    &location.country_en,
                    &location.city_en,
                    &location.district_en,
                )
                .await
        }
    };

    let mut live = core.live.lock().unwrap();
    live.refreshing = false;
    let same_place = live
        .saved
        .location
        .as_ref()
        .is_some_and(|current| current.district_id == location.district_id);
    if !same_place {
        return;
    }
    match result {
        Ok(days) => {
            let saved = Saved {
                location: Some(Location {
                    timezone,
                    ..location
                }),
                days,
            };
            core.store.save(&saved);
            live.replace(saved);
            live.error = None;
        }
        Err(error) => live.error = Some(error.message()),
    }
}

/// Recomputes from the wall clock on every pass, so waking from sleep corrects
/// itself on the next tick instead of having to catch up.
async fn run_ticker(app: AppHandle) {
    let mut last: Option<View> = None;
    loop {
        let now = Utc::now();
        let core = app.state::<Core>();
        let (view, pending) = {
            let mut live = core.live.lock().unwrap();
            let pending = if live.needs_refresh(now) {
                // Set on every attempt, success included: if the server keeps
                // returning a short list this must not become one request a second.
                live.next_attempt = now + Duration::minutes(RETRY_AFTER_MINUTES);
                live.refreshing = true;
                live.saved.location.clone()
            } else {
                None
            };
            (live.view(now), pending)
        };
        if let Some(location) = pending {
            tauri::async_runtime::spawn(refresh(app.clone(), location));
        }
        if last.as_ref() != Some(&view) {
            publish(&app, &view);
            last = Some(view);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Below the icon when it fits (macOS menu bar), above it otherwise (a Windows
/// taskbar along the bottom edge), and never past the screen's sides.
fn popover_origin(
    icon: Area,
    width: f64,
    height: f64,
    screen: Option<Area>,
    margin: f64,
) -> (f64, f64) {
    let centered = icon.x + icon.width / 2.0 - width / 2.0;
    let below = icon.y + icon.height;
    let Some(screen) = screen else {
        return (centered, below);
    };
    let x = centered
        .min(screen.x + screen.width - width - margin)
        .max(screen.x + margin);
    let y = if below + height > screen.y + screen.height {
        icon.y - height
    } else {
        below
    };
    (x, y)
}

fn show_near(window: &WebviewWindow, rect: &Rect) -> tauri::Result<()> {
    let scale = window.scale_factor()?;
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    let outer = window.outer_size()?;
    let screen = window
        .monitor_from_point(position.x, position.y)?
        .map(|monitor| Area {
            x: f64::from(monitor.position().x),
            y: f64::from(monitor.position().y),
            width: f64::from(monitor.size().width),
            height: f64::from(monitor.size().height),
        });
    let icon = Area {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let (x, y) = popover_origin(
        icon,
        f64::from(outer.width),
        f64::from(outer.height),
        screen,
        EDGE_MARGIN * scale,
    );
    window.set_position(PhysicalPosition::new(x, y))?;
    window.show()?;
    window.set_focus()
}

fn toggle(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let core = app.state::<Core>();
    let just_hidden = core
        .hidden_at
        .lock()
        .unwrap()
        .is_some_and(|at| at.elapsed().as_millis() < REOPEN_GUARD_MS);
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else if !just_hidden {
        let _ = show_near(&window, rect);
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let quit = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(TRAY_ICON)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .title("…")
        .tooltip("Ezan Vakti")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle(tray.app_handle(), &rect);
            }
        })
        .build(app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        show_centered(app);
    }));

    builder
        .invoke_handler(tauri::generate_handler![
            get_view,
            list_countries,
            list_cities,
            list_districts,
            set_location
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) => {
                let core = window.state::<Core>();
                *core.hidden_at.lock().unwrap() = Some(Instant::now());
                let _ = window.hide();
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let store = Store::new(app.path().app_data_dir()?);
            let saved = store.load();
            #[cfg(windows)]
            let first_run = saved.location.is_none();
            let live = Live::new(saved, Utc::now());
            app.manage(Core {
                api: Api::new(),
                store,
                live: Mutex::new(live),
                hidden_at: Mutex::new(None),
            });
            build_tray(app.handle())?;
            tauri::async_runtime::spawn(run_ticker(app.handle().clone()));
            #[cfg(windows)]
            if first_run {
                show_centered(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Ezan Vakti başlatılamadı");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(timezone: Option<&str>) -> Location {
        Location {
            country_id: "2".into(),
            country_en: "TÜRKİYE".into(),
            city: "İSTANBUL".into(),
            city_en: "ISTANBUL".into(),
            district_id: "9541".into(),
            district: "İSTANBUL".into(),
            district_en: "ISTANBUL".into(),
            timezone: timezone.map(str::to_string),
        }
    }

    fn live(now: DateTime<Utc>) -> Live {
        let body = include_str!("../tests/fixtures/vakitler-9541.json");
        let saved = Saved {
            location: Some(location(Some("Europe/Istanbul"))),
            days: diyanet::parse_times(body).unwrap(),
        };
        Live::new(saved, now)
    }

    fn instant(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn area(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn popover_opens_below_a_top_menu_bar_and_stays_on_screen() {
        let screen = Some(area(0.0, 0.0, 1920.0, 1080.0));
        let middle = area(1000.0, 0.0, 100.0, 24.0);
        assert_eq!(
            popover_origin(middle, 320.0, 440.0, screen, 8.0),
            (890.0, 24.0)
        );
        let corner = area(1880.0, 0.0, 40.0, 24.0);
        assert_eq!(
            popover_origin(corner, 320.0, 440.0, screen, 8.0),
            (1592.0, 24.0)
        );
    }

    #[test]
    fn popover_opens_above_a_bottom_taskbar() {
        let screen = Some(area(0.0, 0.0, 1920.0, 1080.0));
        let icon = area(1700.0, 1040.0, 32.0, 40.0);
        assert_eq!(
            popover_origin(icon, 320.0, 440.0, screen, 8.0),
            (1556.0, 600.0)
        );
    }

    #[test]
    fn popover_respects_a_display_with_negative_coordinates() {
        let screen = Some(area(-1920.0, 0.0, 1920.0, 1080.0));
        let icon = area(-1910.0, 0.0, 20.0, 24.0);
        assert_eq!(
            popover_origin(icon, 320.0, 440.0, screen, 8.0),
            (-1912.0, 24.0)
        );
    }

    #[test]
    fn no_location_asks_for_one() {
        let view = Live::new(Saved::default(), Utc::now()).view(Utc::now());
        assert_eq!(view.title, "Konum seç");
        assert!(view.rows.is_empty());
        assert_eq!(view.tray_label, None);
    }

    #[test]
    fn real_data_produces_a_title_and_marks_the_current_row() {
        let view = live(Utc::now()).view(instant("2026-11-05T10:00:00Z"));
        assert_eq!(view.location.as_deref(), Some("İSTANBUL"));
        assert_eq!(view.rows.len(), 6);
        let current: Vec<_> = view.rows.iter().filter(|row| row.current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].name, "Öğle");
        assert!(view.title.starts_with("İkindi'ye "), "{}", view.title);
    }

    #[test]
    fn expired_data_shows_no_time_rather_than_a_wrong_one() {
        let view = live(Utc::now()).view(instant("2027-01-15T10:00:00Z"));
        assert_eq!(view.title, "Vakit yok");
        assert!(view.rows.is_empty());
    }

    #[test]
    fn refresh_triggers_only_near_the_end_of_the_data() {
        let plenty = instant("2026-11-05T10:00:00Z");
        let low = instant("2026-11-25T10:00:00Z");
        assert!(!live(plenty).needs_refresh(plenty));
        assert!(live(low).needs_refresh(low));

        let mut retrying = live(low);
        retrying.next_attempt = low + Duration::minutes(RETRY_AFTER_MINUTES);
        assert!(!retrying.needs_refresh(low));
        retrying.next_attempt = low;
        retrying.refreshing = true;
        assert!(!retrying.needs_refresh(low));
    }

    #[test]
    fn an_unresolved_zone_is_reported_as_guessed() {
        let mut state = live(Utc::now());
        let mut saved = state.saved.clone();
        saved.location = Some(location(None));
        state.replace(saved);
        assert!(state.view(instant("2026-11-05T10:00:00Z")).timezone_guessed);
    }
}
