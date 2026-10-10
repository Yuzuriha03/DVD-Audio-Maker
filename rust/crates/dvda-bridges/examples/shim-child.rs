fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    println!("{}", arguments.join("|"));
    eprintln!("shim-child-stderr");
    if arguments.iter().any(|argument| argument == "wait") {
        std::fs::write("child.pid", std::process::id().to_string()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    std::process::exit(37);
}
