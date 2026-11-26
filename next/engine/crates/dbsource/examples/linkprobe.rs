// Link probe for the portability gate: every driver linked into one small program.
fn main() {
    let pg = postgres::Config::new();
    let my = mysql::OptsBuilder::new();
    let lite = rusqlite::Connection::open_in_memory().map(|c| c.is_autocommit()).unwrap_or(false);
    let provider = rustls::crypto::ring::default_provider();
    println!("{} {:?} {lite} {}", pg.get_hosts().len(), mysql::Opts::from(my).get_user().map(str::len), provider.cipher_suites.len());
}
