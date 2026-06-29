fn main() {
    let data = std::fs::read("assets/orbital.elf.lz4").unwrap();
    let decompressed = lz4_flex::decompress_size_prepended(&data).unwrap();
    std::fs::write("assets/orbital.elf", &decompressed).unwrap();
    eprintln!("Decompressed {} -> {} bytes", data.len(), decompressed.len());
}
