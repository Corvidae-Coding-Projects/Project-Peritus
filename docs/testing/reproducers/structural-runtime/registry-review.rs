use peritus_product_state::publication::PublishedRecords;
use std::{collections::BTreeMap, hint::black_box, time::Instant};
fn main() {
    for n in [1u64, 64, 1024, 16384] {
        let r = PublishedRecords::new((0..n).map(|key| (key, key)).collect::<BTreeMap<_, _>>());
        let count = if n > 1024 { 200 } else { 2000 };
        let t = Instant::now();
        for _ in 0..count {
            black_box(r.read().unwrap().get(&0));
        }
        println!(
            "{{\"records\":{n},\"queries\":{count},\"mean_us\":{}}}",
            t.elapsed().as_secs_f64() * 1e6 / f64::from(count)
        );
    }
}
