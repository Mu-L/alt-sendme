use tauri::{LogicalSize, Size, WebviewWindow};

const DEFAULT_WIDTH: f64 = 1024.0;
const DEFAULT_HEIGHT: f64 = 680.0;
const DPI_COMPENSATION: f64 = 0.25;

fn dpi_adjustment(scale_factor: f64) -> f64 {
    1.0 + (scale_factor - 1.0) * DPI_COMPENSATION
}

pub fn adjust_initial_window_size(window: &WebviewWindow) {
    let monitor = match window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    {
        Some(monitor) => monitor,
        None => {
            tracing::warn!("No monitor available; skipping Windows DPI window adjustment");
            return;
        }
    };

    let scale_factor = monitor.scale_factor();
    if scale_factor <= 1.0 {
        return;
    }

    let adjustment = dpi_adjustment(scale_factor);

    let mut width = DEFAULT_WIDTH / adjustment;
    let mut height = DEFAULT_HEIGHT / adjustment;

    let work_area = monitor.work_area();
    let max_width = work_area.size.width as f64 / scale_factor * 0.92;
    let max_height = work_area.size.height as f64 / scale_factor * 0.88;
    width = width.min(max_width);
    height = height.min(max_height);

    let zoom = 1.0 / adjustment;

    if let Err(error) = window.set_zoom(zoom) {
        tracing::warn!(
            error = %error,
            "Failed to adjust webview zoom for Windows DPI scaling"
        );
    }

    if let Err(error) = window.set_size(Size::Logical(LogicalSize::new(width, height))) {
        tracing::warn!(
            error = %error,
            "Failed to adjust window size for Windows DPI scaling"
        );
    } else {
        tracing::debug!(
            scale_factor,
            adjustment,
            width,
            height,
            zoom,
            "Adjusted initial window size and zoom for Windows DPI scaling"
        );
    }
}
