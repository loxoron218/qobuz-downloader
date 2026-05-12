use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        ToolbarView,
        gtk::{Box, Button, DropDown, Label, Orientation::Vertical},
        prelude::{BoxExt, WidgetExt},
    },
    parking_lot::Mutex,
};
