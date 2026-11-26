#[path = "../tests/support/inventory.rs"]
mod inventory;

fn main() {
    for key in inventory::inventory(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
        println!("{key}");
    }
}
