//! Rain as a dither texture: one band per forecast hour, streaked on a
//! slant, as dense as the rain is likely and heavy. Dry hours stay paper.

use ferrite_design::dither::Picture;

use crate::forecast::Hour;

/// Samples across one hour's band.
const BAND: u32 = 8;
const HEIGHT: u32 = 40;

/// How hard it rains in an hour, 0–1: mostly the chance, lifted by amount.
pub fn intensity(hour: &Hour, heavy: f32) -> f32 {
    let chance = (hour.chance / 100.).clamp(0., 1.);
    let amount = (hour.amount / heavy).clamp(0., 1.);
    (chance * 0.7 + amount * 0.3).max(if hour.amount > 0. { 0.25 } else { 0. }).clamp(0., 1.)
}

/// A cheap, stable per-column jitter so streaks don't line up.
fn jitter(x: u32) -> f32 {
    let h = x.wrapping_mul(2_654_435_761) >> 16;
    (h & 0xff) as f32 / 255.
}

/// The texture for `hours`, `BAND` samples per hour wide. `heavy` is what
/// counts as a downpour in the forecast's units (mm or in per hour).
pub fn picture(hours: &[Hour], heavy: f32) -> Picture {
    let n = hours.len().max(1) as u32;
    let w = n * BAND;
    let levels = (0..HEIGHT)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let Some(hour) = hours.get((x / BAND) as usize) else { return 0 };
            let i = intensity(hour, heavy);
            if i <= 0. {
                return 0;
            }
            // Slanted streaks: a sawtooth along x + y/2, offset per column.
            let phase = ((x as f32 + y as f32 * 0.5) / 5. + jitter(x) * 0.6).fract();
            let streak = phase < 0.3 + 0.3 * i;
            let level = if streak { 0.35 + 0.65 * i } else { 0.15 * i };
            (level.clamp(0., 1.) * 255.).round() as u8
        })
        .collect();
    Picture::new(w, HEIGHT, levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hour(chance: f32, amount: f32) -> Hour {
        Hour { time: String::new(), temp: 0., chance, amount }
    }

    fn ink(p: &Picture) -> f32 {
        let (w, h) = p.size();
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| p.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)).sum()
    }

    #[test]
    fn dry_hours_stay_blank() {
        assert_eq!(ink(&picture(&[hour(0., 0.), hour(0., 0.)], 4.)), 0.);
    }

    #[test]
    fn more_rain_is_more_ink() {
        let light = ink(&picture(&[hour(20., 0.)], 4.));
        let heavy = ink(&picture(&[hour(90., 6.)], 4.));
        assert!(light > 0.);
        assert!(heavy > light * 2.);
    }

    #[test]
    fn measured_rain_shows_even_at_low_chance() {
        assert!(intensity(&hour(0., 0.2), 4.) >= 0.25);
        assert_eq!(intensity(&hour(100., 100.), 4.), 1.);
    }

    #[test]
    fn one_band_per_hour() {
        assert_eq!(picture(&vec![hour(0., 0.); 24], 4.).size(), (24 * BAND, HEIGHT));
    }
}
