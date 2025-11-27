use libadwaita::{
    Clamp, PreferencesGroup,
    gtk::{
        Align::{Center, Start},
        Box, Label,
        Orientation::Vertical,
    },
    prelude::{
        AdwApplicationWindowExt, BoxExt, CheckButtonExt, EntryRowExt, PreferencesGroupExt,
        PreferencesRowExt, WidgetExt,
    },
};

use crate::ui::login::LoginWindow;

impl LoginWindow {
    /// Sets up the widget hierarchy and layout for the login window.
    ///
    /// This private method constructs the complete UI layout including:
    /// - Title and subtitle labels
    /// - Authentication method selection radio buttons
    /// - Credential input sections (email/password and user ID/token)
    /// - Login button with appropriate styling
    /// - Proper spacing and alignment using Libadwaita components
    ///
    /// The method uses Libadwaita's [`Clamp`] for responsive width constraints
    /// and organizes content in a vertical [`Box`] with appropriate margins and spacing.
    pub(crate) fn setup_widgets(&self) {
        let main_clamp = Clamp::builder().maximum_size(400).build();

        let main_box = Box::new(Vertical, 24);
        main_box.set_margin_top(32);
        main_box.set_margin_bottom(32);
        main_box.set_margin_start(24);
        main_box.set_margin_end(24);

        // Logo/Title area
        let logo_label = Label::new(Some("Qobuz"));
        logo_label.add_css_class("title-1");
        logo_label.set_halign(Center);
        main_box.append(&logo_label);

        // Subtitle
        let subtitle_label = Label::new(Some("Enter your credentials to get started"));
        subtitle_label.add_css_class("subtitle");
        subtitle_label.set_halign(Center);
        main_box.append(&subtitle_label);

        // Spacer
        let spacer = Box::new(Vertical, 0);
        spacer.set_vexpand(true);
        main_box.append(&spacer);

        // Credential type selection with better spacing
        let selection_label = Label::new(Some("Authentication Method"));
        selection_label.set_halign(Start);
        selection_label.add_css_class("heading");
        main_box.append(&selection_label);

        // Radio buttons for credential type with proper styling
        self.email_radio.set_active(true); // Default to email/password

        let radio_box = Box::new(Vertical, 8);
        radio_box.append(&self.email_radio);
        radio_box.append(&self.token_radio);
        main_box.append(&radio_box);

        // Create credential input sections using AdwPreferencesGroup for consistent styling
        let email_preferences_group = PreferencesGroup::builder()
            .title("Email/Username and Password")
            .build();

        // Configure the email entry row
        self.email_entry_row.set_title("Email or Username");
        self.email_entry_row.set_show_apply_button(false);
        email_preferences_group.add(&self.email_entry_row);

        // Add password entry row to the same preferences group
        email_preferences_group.add(&self.password_entry);

        let email_section = Box::new(Vertical, 16);
        email_section.append(&email_preferences_group);

        // Token section with similar structure
        let token_preferences_group = PreferencesGroup::builder()
            .title("User ID and Auth Token")
            .build();

        // Configure the user ID entry row
        self.user_id_entry_row.set_title("User ID");
        self.user_id_entry_row.set_show_apply_button(false);
        token_preferences_group.add(&self.user_id_entry_row);

        token_preferences_group.add(&self.auth_token_entry);

        let token_section = Box::new(Vertical, 16);
        token_section.append(&token_preferences_group);

        // Add sections to stack
        self.credential_stack
            .add_named(&email_section, Some("email"));
        self.credential_stack
            .add_named(&token_section, Some("token"));
        self.credential_stack.set_visible_child_name("email");

        main_box.append(&self.credential_stack);

        // Login button with proper spacing
        let button_box = Box::new(Vertical, 0);
        button_box.set_margin_top(16);
        button_box.append(&self.login_button);
        main_box.append(&button_box);

        // Spacer at bottom
        let bottom_spacer = Box::new(Vertical, 0);
        bottom_spacer.set_vexpand(true);
        main_box.append(&bottom_spacer);

        // Set content with proper structure
        main_clamp.set_child(Some(&main_box));
        self.toast_overlay.set_child(Some(&main_clamp));
        self.window.set_content(Some(&self.toast_overlay));
    }
}
