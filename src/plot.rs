use crate::smartstate::{Container, Smartstate};
use crate::ui::{GuiResult, Interaction, Response, Ui, Widget};
use core::range::Range;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use heapless::Vec;

const MAX_TICKS: usize = 64;

fn bounds_of(data: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    data.iter().fold(
        (data[0].0, data[0].0, data[0].1, data[0].1),
        |(mx, xmax, my, ymax), &(x, y)| (mx.min(x), xmax.max(x), my.min(y), ymax.max(y)),
    )
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Viewport {
    FitAll,
    FitAllSquare,
    FitFirst(usize),
    FitFirstSquare(usize),
    FitLast(usize),
    FitLastSquare(usize),
    FitXRange(usize, usize),
    FitXRangeSquare(usize, usize),
    FitYRange(usize, usize),
    FitYRangeSquare(usize, usize),
    Range {
        x: Range<f32>,
        y: Range<f32>,
    },
    CenterExtent {
        center: (f32, f32),
        x_extent: f32,
        y_extent: f32,
    },
    CenterExtentSquareX {
        center: (f32, f32),
        x_extent: f32,
    },
    CenterExtentSquareY {
        center: (f32, f32),
        y_extent: f32,
    },
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TickStrategy {
    EveryN(f32),
    PowerOfN(f32),
    MultipleOfN(f32),
    StepPattern,
    DataPoints,
}
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TickSide {
    Center,
    Positive,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GridOptions {
    Off,
    MajorOnly,
    All,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TickOptions {
    pub strategy: TickStrategy,
    pub height: u32,
    pub show_minor: bool,
    pub minor_height: u32,
    pub dim_minor: bool,
    pub major_every: usize,
    pub side: TickSide,
    pub min_distance_px: f32,
    pub grid: GridOptions,
}

impl Default for TickOptions {
    fn default() -> Self {
        Self {
            strategy: TickStrategy::PowerOfN(10.0),
            height: 4,
            show_minor: true,
            minor_height: 2,
            dim_minor: true,
            major_every: 5,
            side: TickSide::Center,
            min_distance_px: 25.0,
            grid: GridOptions::Off,
        }
    }
}

pub struct PanState {
    pub x: f32,
    pub y: f32,
    pub drag_start: Option<Point>,
}

pub struct RangeLabelOptions<'a> {
    pub font: MonoFont<'a>,
    pub decimals: usize,
}

pub struct Plot<'a> {
    data: &'a [(f32, f32)],
    #[allow(dead_code)]
    smartstate: Container<'a, Smartstate>,
    width: u32,
    height: u32,
    show_x_axis: bool,
    show_y_axis: bool,
    show_border: bool,
    range_labels: Option<RangeLabelOptions<'a>>,
    viewport: Viewport,
    x_ticks: TickOptions,
    y_ticks: TickOptions,
    pan: Option<&'a mut PanState>,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport::FitAll
    }
}

impl Viewport {
    fn compute_bounds(
        &self,
        data: &[(f32, f32)],
        screen_w: u32,
        screen_h: u32,
    ) -> (f32, f32, f32, f32) {
        let full_bounds = bounds_of(data);

        let subset = |r: Range<usize>| -> &[(f32, f32)] {
            let start = r.start.min(data.len());
            let end = r.end.min(data.len()).max(start);
            &data[start..end]
        };

        let (mut min_x, mut max_x, mut min_y, mut max_y) = match self {
            Viewport::FitAll => full_bounds,
            Viewport::FitAllSquare => apply_square(full_bounds, screen_w, screen_h),
            Viewport::FitFirst(n) => {
                let s = subset((0..*n).into());
                bounds_of(if s.len() < 2 { data } else { s })
            }
            Viewport::FitFirstSquare(n) => {
                let s = subset((0..*n).into());
                apply_square(
                    bounds_of(if s.len() < 2 { data } else { s }),
                    screen_w,
                    screen_h,
                )
            }
            Viewport::FitLast(n) => {
                let start = data.len().saturating_sub(*n);
                let s = subset((start..data.len()).into());
                bounds_of(if s.len() < 2 { data } else { s })
            }
            Viewport::FitLastSquare(n) => {
                let start = data.len().saturating_sub(*n);
                let s = subset((start..data.len()).into());
                apply_square(
                    bounds_of(if s.len() < 2 { data } else { s }),
                    screen_w,
                    screen_h,
                )
            }
            Viewport::FitXRange(start, end) => {
                let s = subset((*start..*end).into());
                let (mx, xmax, _, _) = bounds_of(if s.len() < 2 { data } else { s });
                let (_, _, my, ymax) = full_bounds;
                (mx, xmax, my, ymax)
            }
            Viewport::FitXRangeSquare(start, end) => {
                let s = subset((*start..*end).into());
                let (mx, xmax, _, _) = bounds_of(if s.len() < 2 { data } else { s });
                let (_, _, my, ymax) = full_bounds;
                apply_square((mx, xmax, my, ymax), screen_w, screen_h)
            }
            Viewport::FitYRange(start, end) => {
                let (mx, xmax, _, _) = full_bounds;
                let s = subset((*start..*end).into());
                let (_, _, my, ymax) = bounds_of(if s.len() < 2 { data } else { s });
                (mx, xmax, my, ymax)
            }
            Viewport::FitYRangeSquare(start, end) => {
                let (mx, xmax, _, _) = full_bounds;
                let s = subset((*start..*end).into());
                let (_, _, my, ymax) = bounds_of(if s.len() < 2 { data } else { s });
                apply_square((mx, xmax, my, ymax), screen_w, screen_h)
            }
            Viewport::Range { x, y } => (x.start, x.end, y.start, y.end),
            Viewport::CenterExtent {
                center,
                x_extent,
                y_extent,
            } => {
                let half_x = x_extent / 2.0;
                let half_y = y_extent / 2.0;
                (
                    center.0 - half_x,
                    center.0 + half_x,
                    center.1 - half_y,
                    center.1 + half_y,
                )
            }
            Viewport::CenterExtentSquareX { center, x_extent } => {
                let half_x = x_extent / 2.0;
                let y_extent = x_extent * screen_h as f32 / screen_w as f32;
                let half_y = y_extent / 2.0;
                (
                    center.0 - half_x,
                    center.0 + half_x,
                    center.1 - half_y,
                    center.1 + half_y,
                )
            }
            Viewport::CenterExtentSquareY { center, y_extent } => {
                let half_y = y_extent / 2.0;
                let x_extent = y_extent * screen_w as f32 / screen_h as f32;
                let half_x = x_extent / 2.0;
                (
                    center.0 - half_x,
                    center.0 + half_x,
                    center.1 - half_y,
                    center.1 + half_y,
                )
            }
        };

        if max_x - min_x <= 0.0 {
            let mid = min_x;
            min_x = mid - 1.0;
            max_x = mid + 1.0;
        }
        if max_y - min_y <= 0.0 {
            let mid = min_y;
            min_y = mid - 1.0;
            max_y = mid + 1.0;
        }

        (min_x, max_x, min_y, max_y)
    }
}

fn apply_square(
    (min_x, max_x, min_y, max_y): (f32, f32, f32, f32),
    screen_w: u32,
    screen_h: u32,
) -> (f32, f32, f32, f32) {
    let dx = max_x - min_x;
    let dy = max_y - min_y;
    if dx <= 0.0 || dy <= 0.0 || screen_w == 0 || screen_h == 0 {
        return (min_x, max_x, min_y, max_y);
    }
    let data_aspect = dx / dy;
    let screen_aspect = screen_w as f32 / screen_h as f32;
    if data_aspect > screen_aspect {
        let center_y = (min_y + max_y) / 2.0;
        let new_dy = dx * screen_h as f32 / screen_w as f32;
        (
            min_x,
            max_x,
            center_y - new_dy / 2.0,
            center_y + new_dy / 2.0,
        )
    } else {
        let center_x = (min_x + max_x) / 2.0;
        let new_dx = dy * screen_w as f32 / screen_h as f32;
        (
            center_x - new_dx / 2.0,
            center_x + new_dx / 2.0,
            min_y,
            max_y,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct ViewBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

impl ViewBounds {
    fn to_screen_x(&self, x: f32) -> i32 {
        let dx = self.max_x - self.min_x;
        if dx > 0.0 {
            self.left + ((x - self.min_x) / dx * self.width as f32) as i32
        } else {
            self.left + (self.width / 2) as i32
        }
    }

    fn to_screen_y(&self, y: f32) -> i32 {
        let dy = self.max_y - self.min_y;
        if dy > 0.0 {
            self.top + ((self.max_y - y) / dy * self.height as f32) as i32
        } else {
            self.top + (self.height / 2) as i32
        }
    }

    fn right(&self) -> i32 {
        self.left + self.width as i32
    }

    fn bottom(&self) -> i32 {
        self.top + self.height as i32
    }
}

fn clip_line(
    mut x0: f32,
    mut y0: f32,
    mut x1: f32,
    mut y1: f32,
    vb: &ViewBounds,
) -> Option<((f32, f32), (f32, f32))> {
    let outcode = |x: f32, y: f32| -> u8 {
        let mut code = 0u8;
        if x < vb.min_x {
            code |= 1;
        }
        if x > vb.max_x {
            code |= 2;
        }
        if y < vb.min_y {
            code |= 4;
        }
        if y > vb.max_y {
            code |= 8;
        }
        code
    };

    let mut out0 = outcode(x0, y0);
    let mut out1 = outcode(x1, y1);

    loop {
        if out0 == 0 && out1 == 0 {
            return Some(((x0, y0), (x1, y1)));
        }
        if out0 & out1 != 0 {
            return None;
        }

        let out = if out0 != 0 { out0 } else { out1 };
        let (ox, oy) = if out0 != 0 { (x0, y0) } else { (x1, y1) };
        let (ix, iy) = if out0 != 0 { (x1, y1) } else { (x0, y0) };

        let (nx, ny) = if out & 1 != 0 {
            let t = (vb.min_x - ix) / (ox - ix);
            (vb.min_x, iy + t * (oy - iy))
        } else if out & 2 != 0 {
            let t = (vb.max_x - ix) / (ox - ix);
            (vb.max_x, iy + t * (oy - iy))
        } else if out & 4 != 0 {
            let t = (vb.min_y - iy) / (oy - iy);
            (ix + t * (ox - ix), vb.min_y)
        } else {
            let t = (vb.max_y - iy) / (oy - iy);
            (ix + t * (ox - ix), vb.max_y)
        };

        if out0 != 0 {
            x0 = nx;
            y0 = ny;
            out0 = outcode(x0, y0);
        } else {
            x1 = nx;
            y1 = ny;
            out1 = outcode(x1, y1);
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct Tick {
    value: f32,
    is_major: bool,
}

fn fmt_f32<const N: usize>(s: &mut heapless::String<N>, v: f32, decimals: usize) {
    if v.is_nan() {
        s.push_str("NaN").ok();
        return;
    }
    if v.is_infinite() {
        if v < 0.0 {
            s.push('-').ok();
        }
        s.push_str("inf").ok();
        return;
    }
    if v < 0.0 {
        s.push('-').ok();
    }
    let abs = v.abs();
    let int_part = abs.trunc() as u64;
    write_int(s, int_part);
    if decimals > 0 {
        s.push('.').ok();
    }
    let frac = abs.fract();
    let mut remaining = frac;
    for _ in 0..decimals {
        remaining *= 10.0;
        let digit = remaining.trunc() as u8;
        s.push((b'0' + digit.min(9)) as char).ok();
        remaining -= digit as f32;
    }
}

fn write_int<const N: usize>(s: &mut heapless::String<N>, mut v: u64) {
    if v == 0 {
        s.push('0').ok();
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while v > 0 {
        buf[i] = (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    for j in (0..i).rev() {
        s.push((b'0' + buf[j]) as char).ok();
    }
}

fn compute_ticks<const N: usize>(
    strategy: &TickStrategy,
    min_val: f32,
    max_val: f32,
    data: &[(f32, f32)],
    data_index: usize,
    screen_px: f32,
    min_distance_px: f32,
) -> Vec<Tick, N> {
    let span = max_val - min_val;
    if span <= 0.0 {
        return Vec::new();
    }

    match strategy {
        TickStrategy::EveryN(n) => {
            let n = n.abs().max(f32::MIN_POSITIVE);
            let min_data_step = min_distance_px * span / screen_px;
            let n = if n < min_data_step {
                (min_data_step / n).ceil() * n
            } else {
                n
            };
            let start = (min_val / n).floor() * n;
            let count = ((max_val - start) / n).ceil() as usize + 1;
            let major_every = 5; // will be made configurable
            (0..=count)
                .map(|i| {
                    let v = start + i as f32 * n;
                    Tick {
                        value: v,
                        is_major: v >= min_val && v <= max_val && i % major_every == 0,
                    }
                })
                .filter(|t| t.value >= min_val - 1e-6 && t.value <= max_val + 1e-6)
                .collect()
        }
        TickStrategy::PowerOfN(n) => {
            let n = n.abs().max(f32::MIN_POSITIVE);
            let raw_step = span / 5.0;
            let mut s = (raw_step.log(n)).round();
            let min_data_step = min_distance_px * span / screen_px;
            let min_s = if min_data_step > 0.0 {
                min_data_step.log(n).ceil()
            } else {
                0.0
            };
            if s < min_s {
                s = min_s;
            }
            let step = n.powf(s);
            let major_step = n.powf(s + 1.0);
            let start = (min_val / step).floor() * step;
            let count = ((max_val - start) / step).ceil() as usize + 1;
            (0..=count)
                .map(|i| {
                    let v = start + i as f32 * step;
                    let f = (v / major_step).fract().abs();
                    let is_major = f < 1e-3 || f > 1.0 - 1e-3;
                    Tick {
                        value: v,
                        is_major: is_major && v >= min_val && v <= max_val,
                    }
                })
                .filter(|t| t.value >= min_val - 1e-6 && t.value <= max_val + 1e-6)
                .collect()
        }
        TickStrategy::MultipleOfN(n) => {
            let n = n.abs().max(f32::MIN_POSITIVE);
            let raw_step = span / 5.0;
            let mut s = (raw_step / n).round().max(1.0);
            let min_data_step = min_distance_px * span / screen_px;
            let min_s = (min_data_step / n).ceil().max(1.0);
            if s < min_s {
                s = min_s;
            }
            let step = (n * s).max(f32::MIN_POSITIVE);
            let start = (min_val / step).floor() * step;
            let count = ((max_val - start) / step).ceil() as usize + 1;
            let mut ticks: Vec<Tick, N> = (0..=count)
                .map(|i| {
                    let v = start + i as f32 * step;
                    let f = (v / n).fract().abs();
                    Tick {
                        value: v,
                        is_major: (f < 1e-3 || f > 1.0 - 1e-3) && v >= min_val && v <= max_val,
                    }
                })
                .filter(|t| t.value >= min_val - 1e-6 && t.value <= max_val + 1e-6)
                .collect();
            if ticks.iter().filter(|t| t.is_major).count() == 0 && !ticks.is_empty() {
                ticks[0].is_major = true;
            }
            ticks
        }
        TickStrategy::StepPattern => {
            let start_pow = ((span).log10()).ceil() as i32;

            let mut best: Option<(f32, f32)> = None;

            for offset in 0..40 {
                let pow = start_pow - offset / 3;
                if pow < -15 {
                    break;
                }
                let case = offset % 3;
                let (major_interval, divisor) = match case {
                    0 => (10.0_f32.powi(pow), 5.0),
                    1 => (5.0 * 10.0_f32.powi(pow - 1), 5.0),
                    _ => (2.0 * 10.0_f32.powi(pow - 1), 4.0),
                };
                let step = major_interval / divisor;
                let px_spacing = step / span * screen_px;
                if px_spacing >= min_distance_px {
                    best = Some((step, major_interval));
                } else if best.is_some() {
                    break;
                }
            }

            if let Some((step, major_interval)) = best {
                let start = (min_val / step).floor() * step;
                let count = ((max_val - start) / step).ceil() as usize + 1;
                return (0..=count)
                    .take(N)
                    .map(|i| {
                        let v = start + i as f32 * step;
                        let f = (v / major_interval).fract().abs();
                        let is_major = (f < 1e-3 || f > 1.0 - 1e-3)
                            && v >= min_val
                            && v <= max_val;
                        Tick { value: v, is_major }
                    })
                    .filter(|t| t.value >= min_val - 1e-6 && t.value <= max_val + 1e-6)
                    .collect();
            }
            Vec::new()
        }
        TickStrategy::DataPoints => {
            let major_every = 5;
            data.iter()
                .enumerate()
                .map(|(i, &(x, y))| {
                    let v = if data_index == 0 { x } else { y };
                    Tick {
                        value: v,
                        is_major: i % major_every == 0,
                    }
                })
                .filter(|t| t.value >= min_val - 1e-6 && t.value <= max_val + 1e-6)
                .collect()
        }
    }
}

impl<'a> Plot<'a> {
    pub fn new(data: &'a [(f32, f32)]) -> Self {
        Self {
            data,
            smartstate: Container::empty(),
            width: 200,
            height: 100,
            show_x_axis: false,
            show_y_axis: false,
            show_border: false,
            range_labels: None,
            viewport: Viewport::default(),
            x_ticks: TickOptions::default(),
            y_ticks: TickOptions::default(),
            pan: None,
        }
    }

    pub fn pan(mut self, state: &'a mut PanState) -> Self {
        self.pan = Some(state);
        self
    }

    pub fn smartstate(mut self, smartstate: &'a mut Smartstate) -> Self {
        self.smartstate.set(smartstate);
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    pub fn x_axis(mut self, show: bool) -> Self {
        self.show_x_axis = show;
        self
    }

    pub fn y_axis(mut self, show: bool) -> Self {
        self.show_y_axis = show;
        self
    }

    pub fn border(mut self, show: bool) -> Self {
        self.show_border = show;
        self
    }

    pub fn range_labels(mut self, opts: RangeLabelOptions<'a>) -> Self {
        self.range_labels = Some(opts);
        self
    }

    pub fn viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn x_ticks(mut self, opts: TickOptions) -> Self {
        self.x_ticks = opts;
        self
    }

    pub fn y_ticks(mut self, opts: TickOptions) -> Self {
        self.y_ticks = opts;
        self
    }
}

fn draw_ticks_and_grid<COL: PixelColor>(
    ui: &mut Ui<'_, impl DrawTarget<Color = COL>, COL>,
    vb: &ViewBounds,
    opts: &TickOptions,
    is_x_axis: bool,
    data: &[(f32, f32)],
    axis_color: COL,
    dim_color: COL,
    draw_axis_and_ticks: bool,
) {
    let (data_min, data_max, screen_px) = if is_x_axis {
        (vb.min_x, vb.max_x, vb.width as f32)
    } else {
        (vb.min_y, vb.max_y, vb.height as f32)
    };

    let ticks = compute_ticks::<MAX_TICKS>(
        &opts.strategy,
        data_min,
        data_max,
        data,
        if is_x_axis { 0 } else { 1 },
        screen_px,
        opts.min_distance_px,
    );

    let draw_grid = opts.grid != GridOptions::Off;
    let draw_all_grid_lines = opts.grid == GridOptions::All;
    let skip_ticks_covered_by_grid = draw_grid;

    // --- grid lines ---
    if draw_grid {
        for tick in &ticks {
            if !draw_all_grid_lines && !tick.is_major {
                continue;
            }
            if tick.value.abs() < 1e-6 {
                continue;
            }
            let tick_pos = if is_x_axis {
                vb.to_screen_x(tick.value)
            } else {
                vb.to_screen_y(tick.value)
            };
            let stroke_width = if tick.is_major { 2 } else { 1 };
            let style = PrimitiveStyle::with_stroke(axis_color, stroke_width);
            if is_x_axis {
                ui.draw(
                    &Line::new(
                        Point::new(tick_pos, vb.top),
                        Point::new(tick_pos, vb.bottom()),
                    )
                    .into_styled(style),
                )
                .ok();
            } else {
                ui.draw(
                    &Line::new(
                        Point::new(vb.left, tick_pos),
                        Point::new(vb.right(), tick_pos),
                    )
                    .into_styled(style),
                )
                .ok();
            }
        }
    }

    if !draw_axis_and_ticks {
        return;
    }

    // --- axis line ---
    let axis_pos = if is_x_axis {
        vb.to_screen_y(0.0)
    } else {
        vb.to_screen_x(0.0)
    };
    let axis_style = PrimitiveStyle::with_stroke(axis_color, 1);
    if is_x_axis {
        ui.draw(
            &Line::new(
                Point::new(vb.left, axis_pos),
                Point::new(vb.right(), axis_pos),
            )
            .into_styled(axis_style),
        )
        .ok();
    } else {
        ui.draw(
            &Line::new(
                Point::new(axis_pos, vb.top),
                Point::new(axis_pos, vb.bottom()),
            )
            .into_styled(axis_style),
        )
        .ok();
    }

    // --- tick marks ---
    for tick in &ticks {
        let covered_by_grid = skip_ticks_covered_by_grid
            && (draw_all_grid_lines || tick.is_major);

        if covered_by_grid {
            continue;
        }
        if !tick.is_major && !opts.show_minor {
            continue;
        }

        let tick_pos = if is_x_axis {
            vb.to_screen_x(tick.value)
        } else {
            vb.to_screen_y(tick.value)
        };

        let height = if tick.is_major {
            opts.height
        } else {
            opts.minor_height
        } as i32;

        let color = if !tick.is_major && opts.dim_minor {
            dim_color
        } else {
            axis_color
        };

        let style = PrimitiveStyle::with_stroke(color, 1);

        if is_x_axis {
            let (y0, y1) = match opts.side {
                TickSide::Center => (axis_pos - height, axis_pos + height),
                TickSide::Positive => (axis_pos - height, axis_pos),
            };
            let y0 = y0.max(vb.top);
            let y1 = y1.min(vb.bottom());
            ui.draw(
                &Line::new(Point::new(tick_pos, y0), Point::new(tick_pos, y1))
                    .into_styled(style),
            )
            .ok();
        } else {
            let (x0, x1) = match opts.side {
                TickSide::Center => (axis_pos - height, axis_pos + height),
                TickSide::Positive => (axis_pos, axis_pos + height),
            };
            let x0 = x0.max(vb.left);
            let x1 = x1.min(vb.right());
            ui.draw(
                &Line::new(Point::new(x0, tick_pos), Point::new(x1, tick_pos))
                    .into_styled(style),
            )
            .ok();
        }
    }
}

impl Widget for Plot<'_> {
    fn draw<DRAW: DrawTarget<Color = COL>, COL: PixelColor>(
        &mut self,
        ui: &mut Ui<DRAW, COL>,
    ) -> GuiResult<Response> {
        let iresponse = ui.allocate_space(Size::new(self.width, self.height))?;

        if self.data.is_empty() {
            return Ok(Response::new(iresponse));
        }

        let margin = 1u32;
        let left = iresponse.area.top_left.x + margin as i32;
        let top = iresponse.area.top_left.y + margin as i32;
        let w = iresponse.area.size.width.saturating_sub(2 * margin);
        let h = iresponse.area.size.height.saturating_sub(2 * margin);

        let (mut min_x, mut max_x, mut min_y, mut max_y) = self.viewport.compute_bounds(self.data, w, h);

        let data_w = max_x - min_x;
        let data_h = max_y - min_y;

        if let Some(pan) = &mut self.pan {
            match iresponse.interaction {
                Interaction::Click(point) => {
                    pan.drag_start = Some(point);
                }
                Interaction::Drag(point) => {
                    if let Some(start) = pan.drag_start {
                        let sx = point.x - start.x;
                        let sy = point.y - start.y;
                        pan.x -= sx as f32 / w as f32 * data_w;
                        pan.y += sy as f32 / h as f32 * data_h;
                        pan.drag_start = Some(point);
                    }
                }
                Interaction::Release(_) => {
                    pan.drag_start = None;
                }
                _ => {}
            }
            min_x += pan.x;
            max_x += pan.x;
            min_y += pan.y;
            max_y += pan.y;
        }

        let vb = ViewBounds {
            min_x,
            max_x,
            min_y,
            max_y,
            left,
            top,
            width: w,
            height: h,
        };

        ui.start_drawing(&iresponse.area);

        let bg = Rectangle::new(iresponse.area.top_left, iresponse.area.size).into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(ui.style().item_background_color)
                .build(),
        );
        ui.draw(&bg).ok();

        if self.show_border {
            let border_rect = Rectangle::new(iresponse.area.top_left, iresponse.area.size)
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .stroke_color(ui.style().border_color)
                        .stroke_width(1)
                        .build(),
                );
            ui.draw(&border_rect).ok();
        }

        let axis_color = ui.style().border_color;
        let dim_color = axis_color;
        let show_x = self.show_x_axis && min_y <= 0.0 && max_y >= 0.0;
        let show_y = self.show_y_axis && min_x <= 0.0 && max_x >= 0.0;
        if show_x {
            draw_ticks_and_grid(
                ui,
                &vb,
                &self.x_ticks,
                true,
                self.data,
                axis_color,
                dim_color,
                show_x,
            );
        } else {
            draw_ticks_and_grid(
                ui,
                &vb,
                &self.x_ticks,
                true,
                self.data,
                axis_color,
                dim_color,
                false,
            );
        }
        if show_y {
            draw_ticks_and_grid(
                ui,
                &vb,
                &self.y_ticks,
                false,
                self.data,
                axis_color,
                dim_color,
                show_y,
            );
        } else {
            draw_ticks_and_grid(
                ui,
                &vb,
                &self.y_ticks,
                false,
                self.data,
                axis_color,
                dim_color,
                false,
            );
        }

        if let Some(ref opts) = self.range_labels {
            let text_style = MonoTextStyle::new(&opts.font, ui.style().text_color);
            let mut buf = heapless::String::<32>::new();
            fmt_f32(&mut buf, max_y, opts.decimals);
            let mut max_text = Text::new(&buf, Point::new(vb.left, vb.top), text_style);
            max_text.text_style.baseline = Baseline::Top;
            ui.draw(&max_text).ok();

            let mut buf = heapless::String::<32>::new();
            fmt_f32(&mut buf, min_y, opts.decimals);
            let mut min_text = Text::new(
                &buf,
                Point::new(vb.left, vb.bottom() - opts.font.character_size.height as i32),
                text_style,
            );
            min_text.text_style.baseline = Baseline::Top;
            ui.draw(&min_text).ok();
        }

        let line_style = PrimitiveStyle::with_stroke(ui.style().primary_color, 1);

        for window in self.data.windows(2) {
            if let Some(((cx0, cy0), (cx1, cy1))) =
                clip_line(window[0].0, window[0].1, window[1].0, window[1].1, &vb)
            {
                let p0 = Point::new(vb.to_screen_x(cx0), vb.to_screen_y(cy0));
                let p1 = Point::new(vb.to_screen_x(cx1), vb.to_screen_y(cy1));
                ui.draw(&Line::new(p0, p1).into_styled(line_style)).ok();
            }
        }

        ui.finalize()?;

        Ok(Response::new(iresponse))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(
        strategy: &TickStrategy,
        min_val: f32,
        max_val: f32,
        screen_px: f32,
        min_distance_px: f32,
    ) -> Vec<Tick, MAX_TICKS> {
        compute_ticks::<MAX_TICKS>(strategy, min_val, max_val, &[], 0, screen_px, min_distance_px)
    }

    fn tick_values(ts: &[Tick]) -> heapless::Vec<f32, MAX_TICKS> {
        let mut v = heapless::Vec::new();
        for t in ts {
            v.push(t.value).ok();
        }
        v
    }

    fn major_values(ts: &[Tick]) -> heapless::Vec<f32, MAX_TICKS> {
        let mut v = heapless::Vec::new();
        for t in ts {
            if t.is_major {
                v.push(t.value).ok();
            }
        }
        v
    }

    fn approx_eq(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b.iter())
                .all(|(a, b)| (a - b).abs() < 1e-4)
    }

    // ---- EveryN ----

    #[test]
    fn every_n_basic() {
        let ts = ticks(&TickStrategy::EveryN(1.0), 0.0, 10.0, 200.0, 1.0);
        // 0, 1, 2, ..., 10 = 11 ticks
        assert_eq!(ts.len(), 11);
        assert!(approx_eq(&tick_values(&ts), &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]));
        // majors every 5: 0, 5, 10
        assert!(approx_eq(
            &major_values(&ts),
            &[0.0, 5.0, 10.0]
        ));
    }

    #[test]
    fn every_n_offset_range() {
        let ts = ticks(&TickStrategy::EveryN(2.0), 1.0, 9.0, 200.0, 1.0);
        // start = floor(1/2)*2 = 0, then 0, 2, 4, 6, 8, 10 → filter keeps 2, 4, 6, 8
        assert_eq!(ts.len(), 4);
        assert!(approx_eq(&tick_values(&ts), &[2.0, 4.0, 6.0, 8.0]));
    }

    #[test]
    fn every_n_min_distance_bumps_n() {
        // span=10, screen=200px → 1 unit = 20px. min_distance=50px → min_data_step = 50*10/200=2.5
        // N=1 < 2.5 → bump to ceil(2.5/1)*1 = 3
        let ts = ticks(&TickStrategy::EveryN(1.0), 0.0, 10.0, 200.0, 50.0);
        assert_eq!(ts.len(), 4); // 0, 3, 6, 9
        assert!(approx_eq(&tick_values(&ts), &[0.0, 3.0, 6.0, 9.0]));
    }

    // ---- PowerOfN ----

#[test]
    fn power_of_n_10_range_100() {
        // span=100, screen=300, raw_step=20, n=10, s=round(log10(20))=1, step=10
        let ts = ticks(&TickStrategy::PowerOfN(10.0), 0.0, 100.0, 300.0, 1.0);
        assert_eq!(ts.len(), 11);
        assert!(approx_eq(&tick_values(&ts), &[0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0]));
        assert!(approx_eq(&major_values(&ts), &[0.0, 100.0]));
    }

    #[test]
    fn power_of_n_10_range_50() {
        // span=50, raw_step=10, s=round(log10(10))=1, step=10
        let ts = ticks(&TickStrategy::PowerOfN(10.0), 0.0, 50.0, 300.0, 1.0);
        assert_eq!(ts.len(), 6);
        assert!(approx_eq(&tick_values(&ts), &[0.0, 10.0, 20.0, 30.0, 40.0, 50.0]));
    }

    #[test]
    fn power_of_n_min_distance_increases_step() {
        // span=100, screen=100 → 1 unit = 1px. min_distance=25 → min_data_step=25
        // min_s = ceil(log10(25)) = ceil(1.398) = 2. s was 1, now clamped to 2.
        // step = 10^2 = 100, so only 2 ticks at 0 and 100
        let ts = ticks(&TickStrategy::PowerOfN(10.0), 0.0, 100.0, 100.0, 25.0);
        assert!(ts.len() <= 2);
        let vals = tick_values(&ts);
        assert!(approx_eq(&vals.as_slice(), &[0.0]) || approx_eq(&vals.as_slice(), &[0.0, 100.0]));
    }

    #[test]
    fn power_of_n_negative_range() {
        // span=20, raw_step=4, s=round(log10(4))=round(0.602)=1, step=10
        let ts = ticks(&TickStrategy::PowerOfN(10.0), -10.0, 10.0, 300.0, 1.0);
        let vals = tick_values(&ts);
        assert!(approx_eq(&vals.as_slice(), &[-10.0, 0.0, 10.0]));
    }

    // ---- MultipleOfN ----

    #[test]
    fn multiple_of_n_10_range_100() {
        // span=100, raw_step=20, s=round(20/10)=2, step=20
        let ts = ticks(&TickStrategy::MultipleOfN(10.0), 0.0, 100.0, 300.0, 1.0);
        let vals = tick_values(&ts);
        assert!(approx_eq(&vals.as_slice(), &[0.0, 20.0, 40.0, 60.0, 80.0, 100.0]));
    }

    #[test]
    fn multiple_of_n_minor_major() {
        let ts = ticks(&TickStrategy::MultipleOfN(10.0), 0.0, 100.0, 300.0, 1.0);
        // majors at multiples of n=10: 0,20,40,60,80,100 → all are multiples of 10
        let majors = major_values(&ts);
        assert!(approx_eq(&majors.as_slice(), &[0.0, 20.0, 40.0, 60.0, 80.0, 100.0]));
    }

    #[test]
    fn multiple_of_n_min_distance_increases_s() {
        // span=100, screen=100, min_data_step=25. min_s=ceil(25/10)=3. step=30.
        let ts = ticks(&TickStrategy::MultipleOfN(10.0), 0.0, 100.0, 100.0, 25.0);
        let vals = tick_values(&ts);
        // 0, 30, 60, 90 → 4 ticks
        assert_eq!(vals.len(), 4);
        assert!(approx_eq(&vals.as_slice(), &[0.0, 30.0, 60.0, 90.0]));
    }

    // ---- StepPattern ----

    #[test]
    fn step_pattern_range_100_screen_300() {
        // span=100, screen=300, min=1 → finest step=0.5, 128 ticks (capped)
        let ts = ticks(&TickStrategy::StepPattern, 0.0, 100.0, 300.0, 1.0);
        assert_eq!(ts.len(), 128);
    }

    #[test]
    fn step_pattern_range_500_screen_300() {
        // span=500, screen=300, min=25 → finest step=50, 11 ticks
        let ts = ticks(&TickStrategy::StepPattern, 0.0, 500.0, 300.0, 25.0);
        let vals = tick_values(&ts);
        assert_eq!(vals.len(), 11);
        assert!(approx_eq(&vals.as_slice(), &[0.0, 50.0, 100.0, 150.0, 200.0, 250.0, 300.0, 350.0, 400.0, 450.0, 500.0]));
    }

    #[test]
    fn step_pattern_tight_screen_forces_coarser() {
        // span=10, screen=50, min=25 → all candidates px < 25, returns empty
        let ts = ticks(&TickStrategy::StepPattern, 0.0, 10.0, 50.0, 25.0);
        assert!(ts.is_empty());
    }

    #[test]
    fn step_pattern_negative_range() {
        // span=20, screen=300, min=1 → finest step=0.1, 128 ticks (capped)
        let ts = ticks(&TickStrategy::StepPattern, -10.0, 10.0, 300.0, 1.0);
        assert_eq!(ts.len(), 128);
    }

    // ---- DataPoints ----

    #[test]
    fn data_points_x_ticks() {
        let data = [(0.0, 0.0), (1.0, 2.0), (2.0, 3.0), (3.0, 1.0), (4.0, 2.0),
                    (5.0, 0.0), (6.0, 1.0), (7.0, 3.0), (8.0, 2.0), (9.0, 0.0),
                    (10.0, 1.0)];
        let ts = compute_ticks::<MAX_TICKS>(&TickStrategy::DataPoints, 0.0, 10.0, &data, 0, 300.0, 1.0);
        // x coords: 0,1,2,...,10 = 11 ticks
        assert_eq!(ts.len(), 11);
        let vals = tick_values(&ts);
        assert!(approx_eq(&vals.as_slice(), &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]));
        // majors every 5: index 0 and 5 → values 0 and 5
        let majors = major_values(&ts);
        assert!(approx_eq(&majors.as_slice(), &[0.0, 5.0, 10.0]));
    }

    #[test]
    fn data_points_outside_range_filtered() {
        let data = [(0.0, 0.0), (5.0, 10.0), (10.0, 20.0), (15.0, 30.0)];
        let ts = compute_ticks::<MAX_TICKS>(&TickStrategy::DataPoints, 2.0, 12.0, &data, 0, 300.0, 1.0);
        // only 5.0 and 10.0 are in range
        let vals = tick_values(&ts);
        assert!(approx_eq(&vals.as_slice(), &[5.0, 10.0]));
    }

    // ---- General edge cases ----

    #[test]
    fn zero_span_returns_empty() {
        let ts = ticks(&TickStrategy::EveryN(1.0), 5.0, 5.0, 300.0, 1.0);
        assert!(ts.is_empty());
    }

    #[test]
    fn first_tick_not_before_min() {
        // EveryN with N=3, min=0.5, max=10
        // start = floor(0.5/3)*3 = 0*3 = 0
        // ticks: 0,3,6,9 → first in range is 3 (0 < 0.5 is filtered out)
        let ts = ticks(&TickStrategy::EveryN(3.0), 0.5, 10.0, 300.0, 1.0);
        let vals = tick_values(&ts);
        assert!(vals[0] >= 0.5);
        assert!(approx_eq(&vals.as_slice(), &[3.0, 6.0, 9.0]));
    }

    #[test]
    fn last_tick_not_after_max() {
        let ts = ticks(&TickStrategy::EveryN(3.0), 0.0, 8.5, 300.0, 1.0);
        let vals = tick_values(&ts);
        let last = *vals.last().unwrap();
        assert!(last <= 8.5 + 1e-6);
    }
}
