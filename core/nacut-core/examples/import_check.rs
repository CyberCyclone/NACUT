//! Reads the Mac app's old Swift cache (read-only) and prints how many records it holds.
fn main() {
    let path = nacut_core::platform::data_dir().join("log-cache.json");
    match nacut_core::LogCache::import_swift_v2(&path) {
        Some(c) => {
            let total: f64 = {
                let mut recs: Vec<_> = c.records.into_values().collect();
                nacut_core::PriceBook::bundled().price_all(&mut recs);
                recs.iter().map(|r| r.cost).sum()
            };
            println!("old Swift cache: {} checkpoints, all-time ${total:.4}", c.checkpoints.len());
        }
        None => println!("no readable v2 cache at {}", path.display()),
    }
}
