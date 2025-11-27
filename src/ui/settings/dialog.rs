use {
    crate::ui::settings::config::{load_download_path, load_metadata_config, save_env_var},
    libadwaita::{
        ActionRow, ApplicationWindow, PreferencesDialog, PreferencesGroup, PreferencesPage,
        gio::Cancellable,
        glib::Propagation::Proceed,
        gtk::{Align::Center, Button, FileDialog, Label, Switch, pango::EllipsizeMode::End},
        prelude::{
            ActionRowExt, AdwDialogExt, ButtonExt, FileExt, PreferencesDialogExt,
            PreferencesGroupExt, PreferencesPageExt,
        },
    },
};

use crate::ui::settings::config::save_download_path;

/// A settings dialog for configuring Qobuz Downloader application preferences.
///
/// This struct represents the main settings interface that allows users to configure
/// application-wide settings such as download paths and audio quality preferences.
/// It provides a Libadwaita-compliant preferences dialog with proper widget hierarchy
/// and signal handling.
///
/// The dialog includes:
/// - Download path configuration with folder picker integration
/// - Audio quality format selection (persisted across sessions)
///
/// # Examples
///
/// ```rust
/// use libadwaita::ApplicationWindow;
/// use qobuz_downloader_rust::ui::settings::SettingsDialog;
///
/// // Create and present settings dialog
/// let settings_dialog = SettingsDialog::new();
/// settings_dialog.present(&parent_window);
/// ```
#[derive(Clone)]
pub struct SettingsDialog {
    /// The underlying Libadwaita preferences dialog widget.
    pub dialog: PreferencesDialog,
    /// Action row widget displaying the current download path with folder picker.
    pub download_path_row: ActionRow,
    /// Label widget displaying the current download path within the action row.
    pub download_path_label: Label,
}

impl SettingsDialog {
    /// Creates a new `SettingsDialog` instance with default configuration.
    ///
    /// This constructor initializes all UI components, loads current settings from
    /// the configuration file (`.env`), and sets up the complete widget hierarchy
    /// with proper signal connections for user interaction.
    ///
    /// The dialog is pre-configured with:
    /// - Current download path loaded from settings (defaults to "downloads")
    /// - Non-editable download path entry with folder picker button
    /// - Complete preferences page structure with download settings group
    ///
    /// # Returns
    ///
    /// Returns a fully initialized `SettingsDialog` ready for presentation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let settings = SettingsDialog::new();
    /// // Dialog is ready to be presented
    /// ```
    pub fn new() -> Self {
        let dialog = PreferencesDialog::new();

        // Create download path label
        let current_path = load_download_path().unwrap_or_else(|_| "downloads".to_string());
        let download_path_label = Label::builder()
            .label(&current_path)
            .ellipsize(End)
            .xalign(0.0)
            .hexpand(true)
            .build();

        // Create download path action row
        let download_path_row = ActionRow::builder()
            .title("Download Path")
            .subtitle("Select where downloaded files will be saved")
            .build();

        // Add label to row
        download_path_row.add_suffix(&download_path_label);

        // Create browse button
        let browse_button = Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("Browse...")
            .css_classes(["flat"])
            .build();

        download_path_row.add_suffix(&browse_button);

        let settings_dialog = Self {
            dialog,
            download_path_row,
            download_path_label,
        };

        settings_dialog.setup_widgets();
        settings_dialog.setup_signals(&browse_button);

        settings_dialog
    }

    /// Sets up the complete widget hierarchy for the settings dialog.
    ///
    /// This private method constructs the full Libadwaita preferences dialog structure
    /// with two main preference pages and comprehensive settings organization:
    ///
    /// ## General Page
    /// - **Download settings group**: Contains the download path entry row with folder picker integration
    ///
    /// ## Metadata Page
    /// Organizes metadata tagging preferences into five logical groups:
    /// - **Basic Tags**: Track title, album title, artist, album artist, and cover art
    /// - **Track Info**: Track/disc numbering, total counts, and explicit advisory
    /// - **Release Info**: Release dates, label, copyright, UPC/ISRC codes, media type, and URL
    /// - **Credits**: Composer, producer, and involved people metadata
    /// - **Other**: Genre and comment fields
    ///
    /// The method dynamically loads current metadata configuration from the `.env` file
    /// and creates interactive switch controls for each metadata field. Each switch is
    /// automatically connected to persist its state back to the configuration file.
    ///
    /// # Implementation Details
    ///
    /// - Uses Libadwaita's `PreferencesPage`, `PreferencesGroup`, and `ActionRow` components
    /// - Employs a closure helper (`create_switch`) to avoid code duplication for switch creation
    /// - Loads initial switch states from persisted `MetadataConfig`
    /// - Connects switch state changes to automatic `.env` file updates
    /// - Follows GNOME Human Interface Guidelines for proper visual hierarchy and accessibility
    fn setup_widgets(&self) {
        // Create General page
        let general_page = PreferencesPage::builder()
            .title("General")
            .icon_name("emblem-system-symbolic")
            .build();

        // Download settings group
        let download_group = PreferencesGroup::builder()
            .title("Download")
            .description("Configure download location and behavior")
            .build();

        download_group.add(&self.download_path_row);
        general_page.add(&download_group);
        self.dialog.add(&general_page);

        // Create Metadata page
        let metadata_page = PreferencesPage::builder()
            .title("Metadata")
            .icon_name("document-properties-symbolic")
            .build();

        // Load current config
        let config = load_metadata_config();

        // Helper to create a switch row
        let create_switch = |title: &str, active: bool, key: &'static str| {
            let row = ActionRow::builder().title(title).build();
            let switch = Switch::builder().active(active).valign(Center).build();

            switch.connect_state_set(move |_, state| {
                if let Err(e) = save_env_var(key, &state.to_string()) {
                    eprintln!("Failed to save setting {}: {}", key, e);
                }
                // Return false to allow the state change to propagate
                Proceed
            });

            row.add_suffix(&switch);
            row
        };

        // Basic Tags Group
        let basic_group = PreferencesGroup::builder().title("Basic Tags").build();
        basic_group.add(&create_switch(
            "Track Title",
            config.track_title,
            "QOBUZ_TAG_TRACK_TITLE",
        ));
        basic_group.add(&create_switch(
            "Album Title",
            config.album,
            "QOBUZ_TAG_ALBUM",
        ));
        basic_group.add(&create_switch("Artist", config.artist, "QOBUZ_TAG_ARTIST"));
        basic_group.add(&create_switch(
            "Album Artist",
            config.album_artist,
            "QOBUZ_TAG_ALBUM_ARTIST",
        ));
        basic_group.add(&create_switch(
            "Cover Art",
            config.cover_art,
            "QOBUZ_TAG_COVER_ART",
        ));
        metadata_page.add(&basic_group);

        // Track Info Group
        let track_info_group = PreferencesGroup::builder().title("Track Info").build();
        track_info_group.add(&create_switch(
            "Track Number",
            config.track_number,
            "QOBUZ_TAG_TRACK_NUMBER",
        ));
        track_info_group.add(&create_switch(
            "Total Tracks",
            config.track_total,
            "QOBUZ_TAG_TRACK_TOTAL",
        ));
        track_info_group.add(&create_switch(
            "Disc Number",
            config.disc_number,
            "QOBUZ_TAG_DISC_NUMBER",
        ));
        track_info_group.add(&create_switch(
            "Total Discs",
            config.disc_total,
            "QOBUZ_TAG_DISC_TOTAL",
        ));
        track_info_group.add(&create_switch(
            "Explicit Advisory",
            config.explicit,
            "QOBUZ_TAG_EXPLICIT",
        ));
        metadata_page.add(&track_info_group);

        // Release Info Group
        let release_group = PreferencesGroup::builder().title("Release Info").build();
        release_group.add(&create_switch(
            "Release Year",
            config.release_year,
            "QOBUZ_TAG_RELEASE_YEAR",
        ));
        release_group.add(&create_switch(
            "Release Date",
            config.release_date,
            "QOBUZ_TAG_RELEASE_DATE",
        ));
        release_group.add(&create_switch("Label", config.label, "QOBUZ_TAG_LABEL"));
        release_group.add(&create_switch(
            "Copyright",
            config.copyright,
            "QOBUZ_TAG_COPYRIGHT",
        ));
        release_group.add(&create_switch("UPC", config.upc, "QOBUZ_TAG_UPC"));
        release_group.add(&create_switch("ISRC", config.isrc, "QOBUZ_TAG_ISRC"));
        release_group.add(&create_switch(
            "Media Type",
            config.media_type,
            "QOBUZ_TAG_MEDIA_TYPE",
        ));
        release_group.add(&create_switch("URL", config.url, "QOBUZ_TAG_URL"));
        metadata_page.add(&release_group);

        // Credits Group
        let credits_group = PreferencesGroup::builder().title("Credits").build();
        credits_group.add(&create_switch(
            "Composer",
            config.composer,
            "QOBUZ_TAG_COMPOSER",
        ));
        credits_group.add(&create_switch(
            "Producer",
            config.producer,
            "QOBUZ_TAG_PRODUCER",
        ));
        credits_group.add(&create_switch(
            "Involved People",
            config.involved_people,
            "QOBUZ_TAG_INVOLVED_PEOPLE",
        ));
        metadata_page.add(&credits_group);

        // Other Group
        let other_group = PreferencesGroup::builder().title("Other").build();
        other_group.add(&create_switch("Genre", config.genre, "QOBUZ_TAG_GENRE"));
        other_group.add(&create_switch(
            "Comment",
            config.comment,
            "QOBUZ_TAG_COMMENT",
        ));
        metadata_page.add(&other_group);

        self.dialog.add(&metadata_page);
    }

    /// Sets up signal connections for interactive UI elements.
    ///
    /// This private method establishes event handlers for user interactions,
    /// specifically connecting the folder browse button to the folder picker
    /// functionality. It uses proper closure capture to maintain references
    /// to the settings dialog instance for callback execution.
    ///
    /// # Arguments
    ///
    /// * `browse_button` - Reference to the folder browse button widget that
    ///   triggers the folder selection dialog when clicked.
    fn setup_signals(&self, browse_button: &Button) {
        let settings_window = self.clone();

        // Connect browse button to open folder picker
        browse_button.connect_clicked(move |_| {
            settings_window.open_folder_picker();
        });
    }

    /// Opens a native folder picker dialog for selecting the download directory.
    ///
    /// This private method displays a GTK file dialog configured specifically for
    /// folder selection, allowing users to choose their preferred download location.
    /// When a folder is selected, the path is automatically updated in the UI and
    /// persisted to the application configuration file (`.env`).
    ///
    /// The method handles the asynchronous nature of the file dialog using GTK's
    /// callback-based API and includes proper error handling for configuration
    /// save operations.
    ///
    /// # Implementation Details
    ///
    /// - Uses `FileDialog::select_folder()` for native folder selection experience
    /// - Automatically saves the selected path to `.env` configuration
    /// - Updates the UI entry field with the new path immediately
    /// - Logs errors to stderr if configuration saving fails
    fn open_folder_picker(&self) {
        let dialog = FileDialog::builder()
            .title("Select Download Folder")
            .accept_label("Select")
            .build();

        let settings_window = self.clone();
        dialog.select_folder(
            None::<&ApplicationWindow>,
            None::<&Cancellable>,
            move |result| {
                if let Ok(file) = result
                    && let Some(path) = file.path()
                    && let Some(path_str) = path.to_str()
                {
                    settings_window.download_path_label.set_label(path_str);

                    // Automatically save the new path
                    if let Err(e) = save_download_path(path_str) {
                        eprintln!("Failed to save settings: {}", e);
                    }
                }
            },
        );
    }

    /// Presents the settings dialog to the user.
    ///
    /// Displays the configured settings dialog as a modal window over the specified
    /// parent application window. The dialog appears as a Libadwaita preferences
    /// dialog with proper styling and behavior consistent with GNOME HIG guidelines.
    ///
    /// # Arguments
    ///
    /// * `parent` - Reference to the parent `ApplicationWindow` that will serve as
    ///   the modal parent for the settings dialog. The dialog will be centered
    ///   relative to this window and will block interaction with the parent while open.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let settings_dialog = SettingsDialog::new();
    /// settings_dialog.present(&main_window);
    /// ```
    pub fn present(&self, parent: &ApplicationWindow) {
        self.dialog.present(Some(parent));
    }
}
