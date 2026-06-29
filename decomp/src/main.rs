fn main() {
    println!("SYS_EXIT = {}", syscall::number::SYS_EXIT);
    println!("SYS_BRK = {}", syscall::number::SYS_BRK);
    println!("SYS_SIGACTION = {}", syscall::number::SYS_SIGACTION);
    println!("SYS_READ = {}", syscall::number::SYS_READ);
    println!("SYS_WRITE = {}", syscall::number::SYS_WRITE);
    println!("SYS_OPEN = {}", syscall::number::SYS_OPEN);
    println!("SYS_CLOSE = {}", syscall::number::SYS_CLOSE);
    println!("SYS_MUNMAP = {}", syscall::number::SYS_MUNMAP);
}
