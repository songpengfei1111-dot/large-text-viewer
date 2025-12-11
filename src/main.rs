mod gui;

use std::env;

fn main() -> iced::Result {
    // Check for command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        let command = &args[1];
        
        match command.as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--version" | "-v" => {
                println!("Large Text File Viewer v{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                // Assume it's a file path and try to open it
                // For now, just launch the GUI
                // Future enhancement: pre-load the file
            }
        }
    }
    
    // Launch GUI
    gui::run()
}

fn print_help() {
    println!("Large Text File Viewer - Handle text files >10GB");
    println!();
    println!("USAGE:");
    println!("    large-text-viewer [OPTIONS] [FILE]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print this help message");
    println!("    -v, --version    Print version information");
    println!();
    println!("FEATURES:");
    println!("    • Memory-mapped I/O for efficient large file handling");
    println!("    • Viewport rendering (50 lines at a time)");
    println!("    • Parallel search with regex support");
    println!("    • Find and replace with atomic file operations");
    println!("    • Navigate with arrow keys and page up/down");
    println!();
    println!("EXAMPLES:");
    println!("    large-text-viewer                    # Launch GUI");
    println!("    large-text-viewer myfile.txt         # Open file in GUI");
    println!("    large-text-viewer --help             # Show this help");
}
