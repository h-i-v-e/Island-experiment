fn main() {
    if let Err(error) = island_building::studio::run(std::env::args().skip(1)) {
        eprintln!("building-studio: {error}");
        std::process::exit(2);
    }
}
