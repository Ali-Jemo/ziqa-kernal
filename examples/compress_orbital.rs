fn main() {
    let input_path = "gui/orbital-master/target/x86_64-unknown-redox/release/orbital";
    let output_path = "assets/orbital.elf.lz4";
    match std::fs::read(input_path) {
        Ok(data) => {
            let compressed = lz4_flex::compress_prepend_size(&data);
            if std::fs::write(output_path, &compressed).is_ok() {
                println!("Compressed {} -> {} bytes", data.len(), compressed.len());
            } else {
                eprintln!("Failed to write compressed data to {}", output_path);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to read input {}: {}", input_path, e);
            std::process::exit(1);
        }
    }
}
