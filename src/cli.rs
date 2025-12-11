use large_text_viewer::{Editor, FileHandler, SearchEngine};
use std::env;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "view" => {
            if args.len() < 3 {
                eprintln!("Usage: {} view <file> [start_line]", args[0]);
                return Ok(());
            }
            let file_path = &args[2];
            let start_line = if args.len() > 3 {
                args[3].parse().unwrap_or(0)
            } else {
                0
            };
            view_file(file_path, start_line)?;
        }
        "search" => {
            if args.len() < 4 {
                eprintln!("Usage: {} search <file> <query> [--case-sensitive]", args[0]);
                return Ok(());
            }
            let file_path = &args[2];
            let query = &args[3];
            let case_sensitive = args.len() > 4 && args[4] == "--case-sensitive";
            search_file(file_path, query, case_sensitive)?;
        }
        "replace" => {
            if args.len() < 5 {
                eprintln!("Usage: {} replace <file> <search> <replace> [--case-sensitive]", args[0]);
                return Ok(());
            }
            let file_path = &args[2];
            let search = &args[3];
            let replace = &args[4];
            let case_sensitive = args.len() > 5 && args[5] == "--case-sensitive";
            replace_in_file(file_path, search, replace, case_sensitive)?;
        }
        "info" => {
            if args.len() < 3 {
                eprintln!("Usage: {} info <file>", args[0]);
                return Ok(());
            }
            let file_path = &args[2];
            file_info(file_path)?;
        }
        "--help" | "-h" => {
            print_usage();
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Large Text Viewer - CLI Mode");
    println!();
    println!("USAGE:");
    println!("    large-text-cli <command> [options]");
    println!();
    println!("COMMANDS:");
    println!("    view <file> [line]              View file starting at line (default: 0)");
    println!("    search <file> <query> [--case-sensitive]");
    println!("                                     Search for text in file");
    println!("    replace <file> <old> <new> [--case-sensitive]");
    println!("                                     Replace text in file");
    println!("    info <file>                      Show file information");
    println!("    --help, -h                       Show this help");
    println!();
    println!("EXAMPLES:");
    println!("    large-text-cli view myfile.txt 100");
    println!("    large-text-cli search myfile.txt \"hello\"");
    println!("    large-text-cli replace myfile.txt \"old\" \"new\" --case-sensitive");
    println!("    large-text-cli info large_log.txt");
}

fn view_file(path: &str, start_line: usize) -> Result<(), Box<dyn std::error::Error>> {
    let handler = FileHandler::open(path)?;
    let total_lines = handler.total_lines();
    
    println!("File: {}", path);
    println!("Total lines: {}", total_lines);
    println!("Showing lines {}-{}", start_line + 1, (start_line + 50).min(total_lines));
    println!("{}", "=".repeat(80));
    
    let lines = handler.get_viewport_lines(start_line, 50);
    for (i, line) in lines.iter().enumerate() {
        println!("{:6} | {}", start_line + i + 1, line);
    }
    
    println!("{}", "=".repeat(80));
    println!("Press Enter to see next page, 'q' to quit, or line number to jump:");
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(())
}

fn search_file(path: &str, query: &str, case_sensitive: bool) -> Result<(), Box<dyn std::error::Error>> {
    let handler = FileHandler::open(path)?;
    let searcher = SearchEngine::new(handler);
    
    println!("Searching in: {}", path);
    println!("Query: \"{}\" (case {}sensitive)", query, if case_sensitive { "" } else { "in" });
    println!();
    
    let results = searcher.search(query, case_sensitive)?;
    
    if results.is_empty() {
        println!("No matches found.");
    } else {
        println!("Found {} match(es):", results.len());
        println!("{}", "=".repeat(80));
        
        for (i, result) in results.iter().enumerate() {
            println!("Match {} at line {}:", i + 1, result.line_number + 1);
            println!("{:6} | {}", result.line_number + 1, result.line_content);
            
            // Show position indicator
            let prefix = format!("{:6} | ", result.line_number + 1);
            let spaces = " ".repeat(prefix.len() + result.match_start);
            let underline = "^".repeat(result.match_end - result.match_start);
            println!("{}{}", spaces, underline);
            println!();
            
            if i >= 19 {
                println!("... and {} more matches (showing first 20)", results.len() - 20);
                break;
            }
        }
    }
    
    Ok(())
}

fn replace_in_file(path: &str, search: &str, replace: &str, case_sensitive: bool) -> Result<(), Box<dyn std::error::Error>> {
    let handler = FileHandler::open(path)?;
    let editor = Editor::new(handler);
    
    println!("File: {}", path);
    println!("Search: \"{}\"", search);
    println!("Replace: \"{}\"", replace);
    println!("Case {}sensitive", if case_sensitive { "" } else { "in" });
    println!();
    
    print!("This will modify the file. Continue? (y/N): ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    if input.trim().to_lowercase() != "y" {
        println!("Cancelled.");
        return Ok(());
    }
    
    println!("Processing...");
    let count = editor.replace_all(path, search, replace, case_sensitive)?;
    
    println!("✓ Replaced {} occurrence(s)", count);
    
    Ok(())
}

fn file_info(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let handler = FileHandler::open(path)?;
    
    println!("File Information");
    println!("{}", "=".repeat(80));
    println!("Path: {}", path);
    println!("Size: {} bytes ({:.2} MB)", handler.file_size(), handler.file_size() as f64 / 1_048_576.0);
    println!("Total lines: {}", handler.total_lines());
    println!("Average bytes per line: {}", if handler.total_lines() > 0 {
        handler.file_size() / handler.total_lines()
    } else {
        0
    });
    println!("{}", "=".repeat(80));
    
    println!("\nFirst 10 lines:");
    let lines = handler.get_viewport_lines(0, 10);
    for (i, line) in lines.iter().enumerate() {
        let preview = if line.len() > 70 {
            format!("{}...", &line[..70])
        } else {
            line.clone()
        };
        println!("{:6} | {}", i + 1, preview);
    }
    
    Ok(())
}
