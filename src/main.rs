//! Barometer: a Ferrite weather station in one panel.
//!
//! The readings as ASCII gauges, the next 24 hours as a line chart with the
//! rain under it as a dither texture, and the week as a heatmap. Weather comes
//! from Open-Meteo, which needs no key: set a place in Settings (it opens by
//! itself on first launch) and that's all.
//!
//!     cargo run
//!     cargo run -- --settings          # open straight into Settings
//!     cargo run -- --demo              # a made-up week, no network
//!     BAROMETER_LOCATION=Fargo cargo run

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod forecast;
mod rain;
mod settings;

use std::time::Duration;

use chrono::{Local, NaiveDate, Timelike};
use ferrite_design::prelude::*;
use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, IntoElement, KeyBinding, Render, SharedString, Subscription, Task, Window, div, px,
    size,
};

use forecast::{Forecast, Location, Units, whole};
use settings::Settings;

/// How often to ask for the weather, and how soon to retry after a failure.
const EVERY: Duration = Duration::from_secs(15 * 60);
const RETRY: Duration = Duration::from_secs(2 * 60);

const APPEARANCES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("system", "System")];
const FPS: [u32; 4] = [25, 60, 120, 240];

/// Where the numbers came from.
#[derive(Clone, Copy, PartialEq)]
enum Feed {
    /// No location yet, or `--demo`: a made-up week.
    Demo,
    Live,
}

struct Barometer {
    settings: Settings,
    settings_open: bool,
    feed: Feed,
    forecast: Forecast,
    /// Why the live feed isn't showing, when it was asked for.
    note: Option<String>,
    fetching: bool,
    /// The polling loop; replacing it cancels the old one.
    task: Option<Task<()>>,
    location: Entity<TextInput>,
    palette: Entity<CommandPalette>,
    toaster: Entity<Toaster>,
    _location: Subscription,
    _appearance: Subscription,
}

fn demo_now(units: Units) -> Forecast {
    let now = Local::now();
    forecast::demo(now.date_naive(), now.hour(), units)
}

impl Barometer {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();
        let location = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).placeholder("Fargo, or 46.88,-96.79").prompt(">");
            input.set_value(settings.location.clone(), cx);
            input
        });
        let _location = cx.subscribe(&location, |this: &mut Self, input, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Submit) {
                this.settings.location = input.read(cx).value().trim().to_string();
                this.save(cx);
                this.restart(cx);
            }
        });

        let first_run = !Settings::exists() && std::env::var("BAROMETER_LOCATION").is_err();
        let units = settings.units;
        let mut station = Self {
            settings,
            settings_open: first_run || std::env::args().any(|a| a == "--settings"),
            feed: Feed::Demo,
            forecast: demo_now(units),
            note: None,
            fetching: false,
            task: None,
            location,
            palette: cx.new(|cx| CommandPalette::new(window, cx)),
            toaster: cx.new(|_| Toaster::new()),
            _location,
            _appearance: theme::follow_system(window),
        };
        station.apply_look(window, cx);
        // Lodestone hands over its shared look as FERRITE_* variables; when
        // launched that way, those win over the saved settings.
        theme::apply_env(cx);
        station.set_commands(cx);
        station.restart(cx);
        if first_run {
            // Saving now is what makes this happen only once.
            station.save(cx);
            station.location.update(cx, |input, cx| input.focus(window, cx));
        }
        station
    }

    // ── Settings ─────────────────────────────────────────────────────────

    fn apply_look(&self, window: &mut Window, cx: &mut App) {
        let s = &self.settings;
        if let Some(scheme) = schemes::by_key(&s.scheme) {
            theme::set_scheme(scheme, cx);
        }
        let appearance = match s.appearance.as_str() {
            "light" => Appearance::Light,
            "system" => Appearance::System,
            _ => Appearance::Dark,
        };
        theme::set_appearance(appearance, window, cx);
        motion::set_fps(s.fps);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.settings.save() {
            self.toaster.update(cx, |t, cx| t.push(toast("Couldn't save settings").danger().message(err), cx));
        }
        cx.notify();
    }

    /// Change a setting, apply it, save it.
    fn change(&mut self, f: impl FnOnce(&mut Settings), window: &mut Window, cx: &mut Context<Self>) {
        let before = self.settings.units;
        f(&mut self.settings);
        self.apply_look(window, cx);
        if before != self.settings.units {
            self.restart(cx);
        }
        self.save(cx);
    }

    fn set_commands(&self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let run = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let weak = weak.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| f(this, window, cx));
            }
        };
        let mut commands = vec![
            command("Refresh").group("Weather").icon(Icon::Refresh).shortcut("Ctrl+R").on_run(run(|this, _, cx| this.restart(cx))),
            command("Settings").group("Weather").icon(Icon::Sliders).shortcut("Ctrl+,").on_run(run(|this, _, cx| {
                this.settings_open = true;
                cx.notify();
            })),
            command("Metric units").group("Weather").on_run(run(|this, window, cx| this.change(|s| s.units = Units::Metric, window, cx))),
            command("Imperial units").group("Weather").on_run(run(|this, window, cx| this.change(|s| s.units = Units::Imperial, window, cx))),
            command("Dark theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "dark".into(), window, cx))),
            command("Light theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "light".into(), window, cx))),
        ];
        for scheme in SCHEMES {
            let weak = weak.clone();
            commands.push(command(format!("Scheme: {}", scheme.name)).group("Theme").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.change(|s| s.scheme = scheme.key.into(), window, cx));
            }));
        }
        self.palette.update(cx, |p, cx| p.set_commands(commands, cx));
    }

    // ── The feed ─────────────────────────────────────────────────────────

    fn location(&self) -> Option<Location> {
        Location::parse(&self.settings.location).or_else(|| std::env::var("BAROMETER_LOCATION").ok().and_then(|l| Location::parse(&l)))
    }

    /// (Re)start polling with the current settings; falls back to the demo
    /// week when there's no place to ask about.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        let units = self.settings.units;
        let location = if std::env::args().any(|a| a == "--demo") { None } else { self.location() };
        let Some(location) = location else {
            self.feed = Feed::Demo;
            self.forecast = demo_now(units);
            self.note = None;
            self.fetching = false;
            cx.notify();
            return;
        };
        self.fetching = true;
        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
                let loc = location.clone();
                let result = cx.background_executor().spawn(async move { forecast::resolve(&loc).and_then(|place| forecast::fetch(&place, units)) }).await;
                let ok = result.is_ok();
                let alive = this.update(cx, |this, cx| {
                    this.fetching = false;
                    match result {
                        Ok(f) => {
                            this.forecast = f;
                            this.feed = Feed::Live;
                            this.note = None;
                        }
                        // Keep the last good reading through a blip.
                        Err(why) => this.note = Some(why),
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(if ok { EVERY } else { RETRY }).await;
            }
        }));
        cx.notify();
    }

    // ── Views ────────────────────────────────────────────────────────────

    /// How to lay out for the room the window gives. gpui has no zoom, so
    /// a maximized window gets a second column and bigger readouts instead.
    fn layout(window: &Window) -> Layout {
        let vp = window.viewport_size();
        // Title bar, status bar, padding and the panel's header.
        let w = f32::from(vp.width) - 2. * f32::from(space::ROW) - 16.;
        let h = f32::from(vp.height) - f32::from(chrome::TITLE_BAR_HEIGHT) - 24. - 2. * f32::from(space::ROW) - 48.;
        let wide = w >= 1300. && h >= 560.;
        Layout {
            wide,
            gauge_cells: if wide { 28 } else { 20 },
            heat_cell: if wide { (w / 2. / 40.).clamp(14., 22.).floor() } else { 14. },
            // Wide: the chart takes the right column's height under its rule
            // and the rain strip. Narrow: whatever is left under the rest.
            chart_h: if wide { (h - 150.).max(150.) } else { (h - 640.).clamp(150., 320.) },
        }
    }

    /// The big number and what it means.
    fn headline(&self, scale: Scale, window: &mut Window, cx: &App) -> impl IntoElement + use<> {
        let p = palette(cx);
        let f = &self.forecast;
        let u = f.units;
        let today = f.days.first();
        let line = |label: &str, value: String| {
            div()
                .flex()
                .flex_row()
                .gap_2()
                // Sized in font cells, not px: the face snaps to whole device
                // pixels, so at 150% "PRESSURE" is wider than 8 × 8px.
                .child(div().flex_none().w(cells(9. * scale as u32 as f32, window)).whitespace_nowrap().display(scale, window).text_color(hsla(p.fg_dim)).child(label.to_uppercase()))
                .child(div().whitespace_nowrap().display(scale, window).text_color(hsla(p.fg)).child(value.to_uppercase()))
        };
        let mut readings = vec![
            line("feels", format!("{}{}", whole(f.feels), u.temp())),
            line("wind", format!("{:.0} {} {}", f.wind, u.speed(), forecast::compass(f.wind_dir))),
            line("pressure", format!("{:.0} hPa", f.pressure)),
        ];
        if let Some(d) = today {
            readings.insert(1, line("hi / lo", format!("{} / {}{}", whole(d.high), whole(d.low), u.temp())));
            readings.push(line("sun", format!("{} - {}", forecast::clock(&d.sunrise), forecast::clock(&d.sunset))));
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .min_w_0()
            .child(banner("temp", format!("{}{}", whole(f.temp), u.temp())).scale(scale).shadow())
            .child(div().display(scale, window).text_color(hsla(p.fg)).child(f.description().to_uppercase()))
            .child(div().flex().flex_col().gap_1().children(readings))
    }

    /// The percentages, as gauges.
    fn gauges(&self, cells: usize) -> impl IntoElement + use<> {
        let f = &self.forecast;
        let chance = f.next(1).first().map(|h| h.chance).unwrap_or(0.);
        let wind = (f.wind / f.units.wind_max()).clamp(0., 1.);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_none()
            .child(ascii_gauge(f.humidity / 100.).label("humid").cells(cells))
            .child(ascii_gauge(f.cloud / 100.).label("cloud").cells(cells))
            .child(ascii_gauge(chance / 100.).label("rain").cells(cells))
            .child(ascii_gauge(wind).label("gale").cells(cells))
            .child(ascii_gauge(f.daylight().unwrap_or(0.)).label("day").cells(cells))
    }

    /// The next 24 hours: temperature, and rain as texture underneath.
    fn next_day(&self, chart_h: f32, cx: &App) -> impl IntoElement + use<> {
        let p = palette(cx);
        let f = &self.forecast;
        let u = f.units;
        let hours = f.next(24);
        let temps: Vec<f32> = hours.iter().map(|h| h.temp).collect();
        let labels: Vec<String> = hours.iter().map(|h| format!("{} · rain {:.0}%", forecast::clock(&h.time), h.chance)).collect();
        let heavy = if u == Units::Imperial { 0.16 } else { 4. };
        let unit = u.temp();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(line_chart("next-24", temps).labels(labels).height(px(chart_h)).format(move |v| format!("{}{unit}", whole(v))))
            .child(
                div()
                    .h(px(36.))
                    .w_full()
                    .border_1()
                    .border_color(hsla(p.line))
                    .bg(hsla(p.sunken))
                    .child(dither(rain::picture(hours, heavy)).ink(hsla(p.fg_dim)).size_full()),
            )
    }

    /// The week: a 7×24 heatmap of temperature, and a line per day.
    fn week(&self, heat_cell: f32, window: &mut Window, cx: &App) -> impl IntoElement + use<> {
        let p = palette(cx);
        let f = &self.forecast;
        let u = f.units;
        let names: Vec<String> = f
            .days
            .iter()
            .map(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").map(|d| d.format("%a").to_string().to_uppercase()).unwrap_or_default())
            .collect();
        let rows: Vec<_> = f
            .days
            .iter()
            .zip(&names)
            .map(|(d, name)| {
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .h(px(heat_cell))
                    .items_center()
                    .whitespace_nowrap()
                    .display(Scale::X1, window)
                    .child(div().flex_none().w(cells(4., window)).text_color(hsla(p.fg_dim)).child(name.clone()))
                    .child(div().flex_none().w(cells(10., window)).text_color(hsla(p.fg)).child(format!("{:>3} {:>3}{}", whole(d.high), whole(d.low), u.temp())))
                    .child(div().flex_none().w(cells(8., window)).text_color(hsla(p.fg_dim)).child(if d.rain > 0.05 { format!("{:.1}{}", d.rain, u.rain()).to_uppercase() } else { "DRY".into() }))
                    .child(div().min_w_0().overflow_hidden().text_ellipsis().body(text::SM).text_color(hsla(p.fg_dim)).child(forecast::describe(d.code)))
            })
            .collect();
        div()
            .flex()
            .flex_row()
            .gap_6()
            .items_start()
            .child(heatmap(f.week_grid()).row_labels(names.clone()).cell(px(heat_cell)))
            .child(div().flex().flex_col().min_w_0().gap(px(4.)).children(rows))
    }

    fn settings_drawer(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let s = &self.settings;
        let scheme_index = SCHEMES.iter().position(|sc| sc.key == s.scheme);
        let close = {
            let weak = cx.weak_entity();
            move |_: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| {
                    this.settings_open = false;
                    cx.notify();
                });
            }
        };
        let status: SharedString = match (self.feed, &self.note, self.fetching) {
            (_, _, true) => "Fetching…".into(),
            (_, Some(note), _) => note.clone().into(),
            (Feed::Live, None, _) => format!("Live: {} from Open-Meteo.", self.forecast.place).into(),
            (Feed::Demo, None, _) => "Showing a made-up week. Enter a place below for the real weather.".into(),
        };

        drawer("settings")
            .open(self.settings_open)
            .title("Settings")
            .width(px(420.))
            .on_close(close)
            .child(rule(Some("weather"), window, cx))
            .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child(status))
            .child(field("location", "Location").hint("A town, or lat,lon · Enter to apply").stacked().child(self.location.clone()))
            .child(
                field("units", "Units").child(
                    segmented("units-seg")
                        .option("Metric")
                        .option("Imperial")
                        .selected(if s.units == Units::Imperial { 1 } else { 0 })
                        .on_select(cx.listener(|this, i: &usize, window, cx| {
                            this.change(|s| s.units = if *i == 1 { Units::Imperial } else { Units::Metric }, window, cx)
                        })),
                ),
            )
            .child(rule(Some("look"), window, cx))
            .child(
                field("scheme", "Scheme").child(
                    select("scheme-select")
                        .options(SCHEMES.iter().map(|sc| sc.name))
                        .selected(scheme_index)
                        .width(px(220.))
                        .on_change(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.scheme = SCHEMES[*i].key.into(), window, cx))),
                ),
            )
            .child(
                field("appearance", "Appearance").child(
                    APPEARANCES.iter().fold(segmented("appearance-seg"), |seg, (_, label)| seg.option(*label))
                        .selected(APPEARANCES.iter().position(|(k, _)| *k == s.appearance).unwrap_or(0))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.appearance = APPEARANCES[*i].0.into(), window, cx))),
                ),
            )
            .child(
                field("fps", "Refresh rate").hint("25 is the classic stepped look").child(
                    FPS.iter().fold(segmented("fps-seg"), |seg, f| seg.option(f.to_string()))
                        .selected(FPS.iter().position(|f| *f == s.fps).unwrap_or(FPS.len() - 1))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.fps = FPS[*i], window, cx))),
                ),
            )
    }
}

impl Render for Barometer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let f = &self.forecast;
        let meta = match self.feed {
            Feed::Live => format!("{} · {}", f.place, forecast::clock(&f.time)),
            Feed::Demo => "demo week".into(),
        };

        let layout = Self::layout(window);
        let now = div()
            .flex()
            .flex_row()
            .justify_between()
            .items_start()
            .gap_6()
            .child(self.headline(if layout.wide { Scale::X2 } else { Scale::X1 }, window, cx))
            .child(self.gauges(layout.gauge_cells));
        let next = div().flex().flex_col().gap_4().child(rule(Some("next 24 hours"), window, cx)).child(self.next_day(layout.chart_h, cx));
        let week = div().flex().flex_col().gap_4().child(rule(Some("the week"), window, cx)).child(self.week(layout.heat_cell, window, cx));
        let body = if layout.wide {
            div()
                .flex()
                .flex_row()
                .gap_6()
                .child(div().flex().flex_col().gap_4().flex_1().min_w_0().child(now).child(week))
                .child(div().flex_1().min_w_0().child(next))
        } else {
            div().flex().flex_col().gap_4().child(now).child(next).child(week)
        };
        // Scrolls rather than clips when the window is shorter than this.
        let station = panel("Station").meta(meta).flex_1().min_h_0().child(
            scroll_area("station-scroll").flex_1().min_h_0().child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .p_2()
                .child(body)
                .when_some(self.note.clone(), |col, note| {
                    col.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .body(text::SM)
                            .text_color(hsla(p.fg_dim))
                            .child(icon(Icon::Warning).fit(px(14.)).color(hsla(p.warning)))
                            .child(note),
                    )
                }),
            ),
        );

        let gear = Button::new("open-settings").icon(Icon::Sliders).ghost().small().tooltip("Settings · Ctrl+,").on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.settings_open = !this.settings_open;
                cx.notify();
            },
        ));
        let drawer = self.settings_drawer(window, cx);
        let feed = match (self.feed, self.fetching) {
            (_, true) => "FETCHING",
            (Feed::Live, false) => "LIVE",
            (Feed::Demo, false) => "DEMO",
        };

        window_frame().child(power_on_in(
            "power",
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(hsla(p.bg))
                .text_color(hsla(p.fg))
                .body(text::BASE)
                .on_action(cx.listener(|this, _: &TogglePalette, window, cx| this.palette.update(cx, |p, cx| p.toggle(window, cx))))
                .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                    this.settings_open = true;
                    cx.notify();
                }))
                .on_action(cx.listener(|this, _: &Refresh, _, cx| this.restart(cx)))
                .child(self.palette.clone())
                .child(self.toaster.clone())
                .child(title_bar("Barometer").child(gear))
                .child(div().flex().flex_col().flex_1().min_h_0().p(space::ROW).child(station))
                .child(drawer)
                .child(
                    status_bar()
                        .left(feed)
                        .left(self.forecast.units.key().to_uppercase())
                        .right(theme::scheme(cx).name.to_uppercase())
                        .right_live(format!("{}FPS", motion::fps())),
                ),
        ))
    }
}

/// What the window has room for (see `Barometer::layout`).
struct Layout {
    /// Two columns: now and the week left, the next 24 hours right.
    wide: bool,
    gauge_cells: usize,
    heat_cell: f32,
    chart_h: f32,
}

/// `n` display-face cells at `Scale::X1`, on this window's display.
fn cells(n: f32, window: &Window) -> gpui::Pixels {
    display_size(Scale::X1, window) / 2. * n
}

gpui::actions!(barometer, [OpenSettings, Refresh]);

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        ferrite_design::init(Appearance::Dark, cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-p", TogglePalette, None),
            KeyBinding::new("ctrl-,", OpenSettings, None),
            KeyBinding::new("ctrl-r", Refresh, None),
        ]);
        let options = chrome::remembered_window_options("barometer", "Barometer", size(px(1040.), px(860.)), cx);
        cx.open_window(options, |window, cx| {
            chrome::square_corners(window);
            chrome::remember_window("barometer", window, cx);
            chrome::power_off_on_close(window, cx);
            cx.new(|cx| Barometer::new(window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
