//! BitArchive presentation layer.
//!
//! This crate owns the Slint UI. Slint stays presentation-only: it never
//! performs database access, filesystem scanning, HTTP requests, RetroArch
//! process management, or platform integration (ARCHITECTURE.md §37).
//!
//! The desktop composition root reaches this crate through [`run`] only, so
//! Slint component details stay behind this boundary.

/// Generated bindings for the Slint sources under `ui/`.
///
/// Kept private on purpose: the crate exposes [`run`] instead of raw Slint
/// component types.
mod app {
    slint::include_modules!();
}

use slint::ComponentHandle as _;

/// Starts the Slint presentation layer and runs its event loop until the root
/// window is closed.
///
/// # Errors
///
/// Returns a [`slint::PlatformError`] if the platform backend or the root
/// window cannot be created.
pub fn run() -> Result<(), slint::PlatformError> {
    let window = app::AppWindow::new()?;
    window.run()
}

/// Verification for the design-token and theme layer.
///
/// The theme layer has no product surface yet: there is no Settings screen and
/// the bootstrap window deliberately offers no Light/Dark switch. These tests
/// are the internal verification mechanism for it. They run the real bootstrap
/// window on Slint's software-renderer backend, so they need neither a visible
/// window nor a change to the operating system's appearance settings.
///
/// They verify behavior, not literal token values: the token values are an
/// adjustable baseline, while the resolution rules and the light/dark polarity
/// of the roles are the contract.
#[cfg(test)]
mod theme_tests {
    use super::app;
    use slint::language::ColorScheme;
    use slint::platform::software_renderer::{
        MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
    };
    use slint::platform::{PlatformError, WindowAdapter};
    use slint::{ComponentHandle as _, Global as _};
    use std::rc::Rc;

    /// Small render target the bootstrap window is rendered into.
    const WIDTH: u32 = 80;
    const HEIGHT: u32 = 60;

    thread_local! {
        static WINDOW: Rc<MinimalSoftwareWindow> =
            MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    }

    struct ThemeTestPlatform;

    impl slint::platform::Platform for ThemeTestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(WINDOW.with(|window| window.clone()))
        }
    }

    /// Rendered pixel that keeps the resulting RGB value.
    #[derive(Clone, Copy, Default, Debug, PartialEq)]
    struct Rgb {
        r: u8,
        g: u8,
        b: u8,
    }

    impl TargetPixel for Rgb {
        fn blend(&mut self, color: PremultipliedRgbaColor) {
            let inv_alpha = 255u32 - color.alpha as u32;
            self.r = (color.red as u32 + self.r as u32 * inv_alpha / 255).min(255) as u8;
            self.g = (color.green as u32 + self.g as u32 * inv_alpha / 255).min(255) as u8;
            self.b = (color.blue as u32 + self.b as u32 * inv_alpha / 255).min(255) as u8;
        }

        fn from_rgb(r: u8, g: u8, b: u8) -> Self {
            Rgb { r, g, b }
        }
    }

    /// Installs the headless platform and creates the bootstrap window.
    fn bootstrap_window() -> (app::AppWindow, Rc<MinimalSoftwareWindow>) {
        slint::platform::set_platform(Box::new(ThemeTestPlatform)).ok();
        let window = WINDOW.with(|window| window.clone());
        window.set_size(slint::PhysicalSize::new(WIDTH, HEIGHT));
        let ui = app::AppWindow::new().expect("bootstrap window must be creatable");
        ui.show().expect("bootstrap window must be showable");
        (ui, window)
    }

    /// Renders the window and returns the top-left pixel, which shows the
    /// window canvas next to the centered placeholder labels.
    fn canvas_pixel(window: &Rc<MinimalSoftwareWindow>) -> Rgb {
        let mut buffer = vec![Rgb::default(); (WIDTH * HEIGHT) as usize];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(buffer.as_mut_slice(), WIDTH as usize);
        });
        buffer[0]
    }

    fn as_rgb(brush: slint::Brush) -> Rgb {
        let color = brush.color();
        Rgb {
            r: color.red(),
            g: color.green(),
            b: color.blue(),
        }
    }

    /// WCAG relative luminance, used to check the light/dark polarity of the
    /// foreground roles instead of asserting literal color values.
    fn relative_luminance(brush: &slint::Brush) -> f64 {
        fn channel(value: u8) -> f64 {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        let color = brush.color();
        0.2126 * channel(color.red())
            + 0.7152 * channel(color.green())
            + 0.0722 * channel(color.blue())
    }

    fn contrast_ratio(first: &slint::Brush, second: &slint::Brush) -> f64 {
        let (lighter, darker) = {
            let (a, b) = (relative_luminance(first), relative_luminance(second));
            if a >= b { (a, b) } else { (b, a) }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// The default preference is the product preference `System`, which Slint
    /// spells `ColorScheme.unknown`.
    #[test]
    fn default_preference_is_system() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        assert_eq!(
            theme.get_preference(),
            ColorScheme::Unknown,
            "the default appearance preference must be System"
        );
    }

    /// `System` follows the scheme the platform reports: dark when Slint
    /// reports dark, light when it reports light, and back again.
    #[test]
    fn system_follows_the_reported_scheme() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_system_scheme(ColorScheme::Light);
        assert!(!theme.get_dark());
        assert_eq!(theme.get_effective_scheme(), ColorScheme::Light);
        let light_roles = theme.get_colors();

        theme.set_system_scheme(ColorScheme::Dark);
        assert!(theme.get_dark());
        assert_eq!(theme.get_effective_scheme(), ColorScheme::Dark);
        let dark_roles = theme.get_colors();
        assert_ne!(
            light_roles, dark_roles,
            "the resolved roles must change with the scheme"
        );

        theme.set_system_scheme(ColorScheme::Light);
        assert_eq!(
            theme.get_colors(),
            light_roles,
            "resolution must be deterministic"
        );
    }

    /// An explicit preference wins over the reported system scheme.
    #[test]
    fn explicit_preference_wins_over_the_reported_scheme() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_system_scheme(ColorScheme::Dark);
        theme.set_preference(ColorScheme::Light);
        assert!(!theme.get_dark());

        theme.set_system_scheme(ColorScheme::Light);
        theme.set_preference(ColorScheme::Dark);
        assert!(theme.get_dark());
    }

    /// When the platform reports no appearance at all, `System` resolves to the
    /// documented Light fallback instead of guessing.
    #[test]
    fn unreported_system_scheme_falls_back_to_light() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_preference(ColorScheme::Unknown);
        theme.set_system_scheme(ColorScheme::Unknown);
        assert!(!theme.get_dark());
        assert_eq!(theme.get_effective_scheme(), ColorScheme::Light);
    }

    /// The foreground roles keep the correct polarity and a readable contrast
    /// against the canvas in both schemes, so a swapped Light/Dark baseline
    /// cannot pass unnoticed.
    #[test]
    fn both_schemes_keep_readable_foreground_contrast() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_system_scheme(ColorScheme::Light);
        let light = theme.get_colors();
        assert!(relative_luminance(&light.text_primary) < relative_luminance(&light.canvas));
        assert!(
            contrast_ratio(&light.text_primary, &light.canvas) >= 7.0,
            "primary text must be clearly readable on the light canvas"
        );

        theme.set_system_scheme(ColorScheme::Dark);
        let dark = theme.get_colors();
        assert!(relative_luminance(&dark.text_primary) > relative_luminance(&dark.canvas));
        assert!(
            contrast_ratio(&dark.text_primary, &dark.canvas) >= 7.0,
            "primary text must be clearly readable on the dark canvas"
        );

        assert!(
            contrast_ratio(&light.focus_ring, &light.canvas) >= 3.0
                && contrast_ratio(&dark.focus_ring, &dark.canvas) >= 3.0,
            "the focus ring must stay visible on the canvas in both schemes"
        );
    }

    /// The bootstrap window really renders the resolved `canvas` role, in both
    /// schemes, instead of a hardcoded window color.
    #[test]
    fn bootstrap_window_renders_the_resolved_canvas_role() {
        let (ui, window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_system_scheme(ColorScheme::Light);
        let light_canvas = as_rgb(theme.get_colors().canvas);
        assert_eq!(canvas_pixel(&window), light_canvas);

        theme.set_system_scheme(ColorScheme::Dark);
        let dark_canvas = as_rgb(theme.get_colors().canvas);
        assert_ne!(
            light_canvas, dark_canvas,
            "Light and Dark canvases must differ"
        );
        assert_eq!(canvas_pixel(&window), dark_canvas);
    }

    /// Components ask for semantic durations, and Reduced Motion turns those
    /// into the instant duration without touching component code.
    #[test]
    fn reduced_motion_removes_motion_durations() {
        let (ui, _window) = bootstrap_window();
        let theme = app::Theme::get(&ui);

        theme.set_reduced_motion(false);
        assert!(theme.get_motion_fast() > 0);
        assert!(theme.get_motion_normal() > theme.get_motion_fast());
        assert!(theme.get_motion_slow() > theme.get_motion_normal());

        theme.set_reduced_motion(true);
        assert_eq!(theme.get_motion_fast(), 0);
        assert_eq!(theme.get_motion_normal(), 0);
        assert_eq!(theme.get_motion_slow(), 0);
    }
}
