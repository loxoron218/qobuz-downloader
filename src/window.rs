//! Main application window.

use {
    libadwaita::{
        Application, ApplicationWindow, HeaderBar, NavigationPage, NavigationView, ToolbarView,
        glib::{Propagation::Proceed, clone},
        gtk::Label,
        prelude::{AdwApplicationWindowExt, GtkWindowExt, WidgetExt},
    },
    tracing::error,
};

use crate::{app::AppState, preferences::settings::save_settings};

/// Builds and returns the main application window.
pub fn build_window(app: &Application, state: &AppState) -> ApplicationWindow {
    let window = ApplicationWindow::new(app);

    let width;
    let height;
    {
        let settings = state.settings.lock();
        width = settings.window_width;
        height = settings.window_height;
    }
    window.set_default_size(width, height);

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&HeaderBar::new());

    let nav_view = NavigationView::new();
    let placeholder = Label::new(Some("Qobuz Downloader"));
    let nav_page = NavigationPage::new(&placeholder, "Qobuz Downloader");
    nav_view.push(&nav_page);

    toolbar.set_content(Some(&nav_view));
    window.set_content(Some(&toolbar));

    window.connect_close_request(clone!(
        #[strong]
        state,
        move |win| {
            let mut settings = state.settings.lock();
            settings.window_width = win.width();
            settings.window_height = win.height();
            drop(settings);
            let settings = state.settings.lock();
            if let Err(err) = save_settings(&settings) {
                error!(error = %err, "Failed to save settings");
            }
            Proceed
        }
    ));

    window
}
