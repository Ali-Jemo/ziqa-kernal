use std::fs::File;
use std::io::Read;
use std::io::Write;

fn main() {
    let mut f = File::open("assets/orbital.elf.lz4").unwrap();
    let mut data = Vec::new();
    f.read_to_end(&mut data).unwrap();
    let decompressed = lz4_flex::decompress_size_prepended(&data).unwrap();
    let mut out = File::create("orbital_embedded.elf").unwrap();
    out.write_all(&decompressed).unwrap();
}
