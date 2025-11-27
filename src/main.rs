mod ui;

use {
    libadwaita::{
        Application,
        gtk::{
            glib::{ExitCode, Propagation::Proceed},
            init,
        },
        prelude::{ApplicationExt, ApplicationExtManual, WidgetExt},
    },
    qobuz_api_rust::QobuzApiService,
    tokio::main,
};

use crate::ui::{login::LoginWindow, main_window::MainWindow};

/// Main entry point for the Qobuz Downloader application.
///
/// This function initializes the GTK framework, creates the Libadwaita application
/// instance, and sets up the application lifecycle. The application uses a
/// login-first workflow where users must authenticate before accessing the
/// main download interface.
///
/// # Returns
///
/// Returns an [`ExitCode`] indicating the application's termination status.
/// - `ExitCode::SUCCESS` (0) on normal termination
/// - `ExitCode::FAILURE` (1) on initialization errors
///
/// # Panics
///
/// This function will panic if GTK initialization fails, as this indicates
/// a fundamental system issue that prevents the application from running.
#[main]
async fn main() -> ExitCode {
    // Initialize GTK before creating any UI components
    init().expect("Failed to initialize GTK");

    // Create the Libadwaita application with a unique application ID
    // The application ID follows reverse domain name notation
    let app = Application::builder()
        .application_id("com.github.qobuz-downloader-rs")
        .build();

    // Connect the application's activate signal to the UI builder function
    // This ensures the UI is only created when the application is activated
    app.connect_activate(build_ui);

    // Run the application event loop and return the exit code
    app.run()
}

/// Builds and displays the application's user interface.
///
/// This function implements the login-first workflow by:
/// 1. Creating and presenting the login window
/// 2. Setting up a callback for successful authentication
/// 3. Configuring application termination when the login window is closed
///
/// Upon successful login, the login window is hidden and the main application
/// window is displayed with the authenticated Qobuz API service.
///
/// # Arguments
///
/// * `app` - A reference to the Libadwaita [`Application`] instance that owns this UI
///
/// # Implementation Details
///
/// The function uses closures with cloned references to handle the asynchronous
/// nature of GTK signal handling while maintaining access to the necessary UI
/// components and application state.
fn build_ui(app: &Application) {
    // Create the login window as the initial interface
    let login_window = LoginWindow::new(app);

    // Set up the callback that handles successful authentication
    let app_clone = app.clone();
    let login_window_clone = login_window.clone();
    login_window.set_on_login_success(move |service| {
        // Hide the login window instead of destroying it to maintain state
        login_window_clone.window.set_visible(false);

        // Create and present the main application window with the authenticated service
        show_main_window(&app_clone, service);
    });

    // Present the login window to the user as the first interaction
    login_window.present();

    // Configure application termination when the login window is closed
    // This ensures the application quits cleanly if the user closes the login window
    let app_clone = app.clone();
    login_window.connect_close_request(move || {
        app_clone.quit();
        Proceed
    });
}

/// Creates and presents the main application window.
///
/// This function initializes the main Qobuz Downloader interface with the
/// authenticated Qobuz API service, enabling all download and search functionality.
///
/// # Arguments
///
/// * `app` - A reference to the Libadwaita [`Application`] instance
/// * `service` - An authenticated [`QobuzApiService`] instance for API interactions
///
/// # Features Enabled
///
/// With an authenticated service, the main window provides:
/// - URL/ID input for direct downloads
/// - Quality selection (MP3, FLAC Lossless, Hi-Res)
/// - Download queue management
/// - Search functionality for Qobuz content
/// - Settings configuration
fn show_main_window(app: &Application, service: QobuzApiService) {
    // Create the main application window with navigation capabilities
    let main_window = MainWindow::new(app, service);
    main_window.present();
}
