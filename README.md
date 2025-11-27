# qobuz-downloader-rs

A modern, native Qobuz music downloader built with Rust and Libadwaita, providing a seamless GNOME desktop experience for downloading high-quality audio from Qobuz.

![Qobuz Downloader Screenshot](assets/screenshot.png)


## Features

### Authentication
- **Dual Authentication Methods**: 
  - Email/Username + Password (MD5 hashed)
  - User ID + Auth Token (recommended)
- **Automatic Credential Persistence**: Saves credentials to `.env` file for future sessions
- **Secure Credential Handling**: Passwords are never stored in plain text

### Download Capabilities
- **Multiple Quality Options**:
  - MP3 320kbps (Format ID: 5)
  - FLAC Lossless (Format ID: 6) 
  - FLAC Hi-Res 24-bit ≤ 96kHz (Format ID: 7)
  - FLAC Hi-Res 24-bit >96kHz & ≤192kHz (Format ID: 27)
- **Comprehensive Content Support**:
  - Individual tracks
  - Complete albums
  - Playlists
- **Automatic Metadata Embedding**: Uses `lofty` crate to embed comprehensive metadata into downloaded files

### User Interface
- **Login-First Workflow**: Secure authentication before accessing download features
- **Dashboard View**: 
  - URL/ID input for direct downloads
  - Quality selection dropdown
  - Real-time download queue management
- **Search Functionality**:
  - Search across entire Qobuz catalog
  - Filter by Albums, Tracks, or All content
  - Visual results with cover art and metadata
  - One-click download or add-to-queue options
- **Download Queue Management**:
  - Visual progress tracking
  - Individual item cancellation
  - Cancel all functionality
  - Status indicators (Queued, Downloading, Completed, Cancelled, Failed)

### Technical Architecture
- **Modern Rust**: Built with async/await using Tokio runtime
- **GNOME HIG Compliant**: Follows GNOME Human Interface Guidelines for consistent UX
- **Libadwaita Integration**: Native GTK4 + Libadwaita widgets for authentic GNOME look and feel
- **Modular Design**: Clean separation of concerns with dedicated modules for UI, authentication, and download logic

## Prerequisites

### System Dependencies
- **GTK4 Development Libraries**
- **Libadwaita Development Libraries**

#### Ubuntu/Debian:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

#### Fedora/RHEL:
```bash
sudo dnf install gtk4-devel libadwaita-devel
```

#### Arch Linux:
```bash
sudo pacman -S gtk4 libadwaita
```

### Qobuz Account
You need an active Qobuz subscription with download privileges. The application supports two authentication methods:

1. **Token-based (Recommended)**: User ID and Authentication Token
2. **Email-based**: Email/Username and MD5-hashed password

## Installation

### From Source

1. **Clone the repository**:
```bash
git clone https://github.com/your-username/qobuz-downloader-rs.git
cd qobuz-downloader-rs
```

2. **Build the project**:
```bash
cargo build --release
```

3. **Run the application**:
```bash
./target/release/qobuz-downloader-rs
```

### Using Cargo (Development)
```bash
cargo run
```

## Configuration

### Environment Variables
Create a `.env` file in the application directory with your Qobuz credentials:

**Token-based authentication (recommended):**
```env
QOBUZ_USER_ID=your_user_id_here
QOBUZ_USER_AUTH_TOKEN=your_auth_token_here
```

**Email and password authentication:**
```env
QOBUZ_EMAIL=your_email@example.com
QOBUZ_PASSWORD=your_md5_hashed_password_here
```

**Username and password authentication:**
```env
QOBUZ_USERNAME=your_username_here
QOBUZ_PASSWORD=your_md5_hashed_password_here
```

> **Note**: If you don't have a `.env` file, the application will prompt you to enter credentials through the login interface, which will automatically create the `.env` file for future use.

## Usage

### Initial Setup
1. Launch the application
2. Enter your Qobuz credentials in the login window
3. Click "Login" to authenticate

### Dashboard Features
- **URL Input**: Paste Qobuz URLs or IDs directly
- **Quality Selection**: Choose your preferred audio quality
- **Download Button**: Start downloading the entered URL/ID
- **Download Queue**: Monitor active downloads with real-time progress

### Search Functionality
1. Navigate to the Search view
2. Enter your search query
3. Select search scope (Albums, Tracks, or All)
4. Browse results with cover art and metadata
5. Click "Download" for immediate download or "Add to Queue" for batch processing

### Download Queue Management
- **Cancel Individual Downloads**: Click the cancel button next to any queued item
- **Cancel All Downloads**: Use the "Cancel All" button to stop all active downloads
- **Clear Queue**: Remove completed/cancelled items from the queue display

## Project Structure

```
qobuz-downloader-rs/
├── src/
│   ├── main.rs              # Application entry point
│   └── ui/                  # User interface modules
│       ├── login/           # Login window implementation
│       ├── main_window.rs   # Main dashboard and download queue
│       ├── search.rs        # Search functionality
│       └── settings.rs      # Settings management
├── assets/                  # Application assets (icons, images)
├── Cargo.toml               # Project dependencies and metadata
└── README.md                # This documentation file
```

## Dependencies

### Core Dependencies
- **libadwaita**: GTK4-based adaptive UI library for GNOME
- **qobuz-api-rust**: Custom Qobuz API client (included as submodule)
- **tokio**: Async runtime for non-blocking operations
- **regex**: Pattern matching for URL parsing
- **serde**: Serialization/deserialization for API responses

## Development

### Building
```bash
# Development build
cargo build

# Release build  
cargo build --release
```

### Running Tests
```bash
cargo test
```

### Code Formatting
The project uses `rustfmt` with custom configuration:
```bash
cargo fmt
```

## Contributing

Contributions are welcome! Please follow these guidelines:

1. **Fork the repository**
2. **Create a feature branch**: `git checkout -b feature/your-feature`
3. **Commit your changes**: `git commit -am 'Add some feature'`
4. **Push to the branch**: `git push origin feature/your-feature`
5. **Open a pull request**

### Areas for Improvement
- **Testing**: Add comprehensive unit and integration tests
- **Documentation**: Expand inline documentation and user guides
- **Performance**: Optimize download queue processing and memory usage
- **Features**: Add playlist support, advanced metadata options, and batch download capabilities

## License

This project is licensed under the GNU General Public License v3.0 (GPL-3.0). See the [LICENSE](LICENSE) file for details.

## Acknowledgements

- **DJDoubleD**: Original C# Qobuz API and QobuzDownloaderX-MOD implementation that inspired this project
- **GNOME Project**: Libadwaita and GTK4 frameworks
- **Rust Community**: Excellent ecosystem and tooling

## Disclaimer

This application is an unofficial client for the Qobuz music streaming service. Use it responsibly and in accordance with Qobuz's terms of service. The developers are not affiliated with Qobuz and are not responsible for any misuse of this software.

**Warning**: Web player credential extraction may break at any time due to updates to the Qobuz Web Player.
