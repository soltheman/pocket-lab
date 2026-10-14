//! Lists what a script set still lacks before it can install: modules that fail to parse
//! and calls nothing resolves, grouped and counted.
//!
//!   cargo run -p sim --example gsc_census -- <scripts dir> <iw4|t5> <root module>...
//!
//! The scripts dir holds `<module>.gsc` files, e.g. extracted from a game's zones.

use sim::script::{Catalog, FileSources, Program};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, realm, roots @ ..] = args.as_slice() else {
        eprintln!("usage: gsc_census <scripts dir> <iw4|t5> <root module>...");
        std::process::exit(2);
    };
    let catalog = match realm.as_str() {
        "iw4" => Catalog::iw4(),
        "t5" => Catalog::t5(),
        other => {
            eprintln!("unknown realm {other} (iw4|t5)");
            std::process::exit(2);
        }
    };
    let roots: Vec<&str> = roots.iter().map(String::as_str).collect();
    let census = Program::census(&FileSources(dir.into()), &roots, &catalog);
    println!(
        "modules parsed: {}  functions: {}  module faults: {}  unresolved names: {} ({} call sites)",
        census.modules,
        census.functions,
        census.module_faults.len(),
        census.unresolved.len(),
        census.unresolved.values().sum::<usize>()
    );
    println!("\n== module faults");
    for fault in &census.module_faults {
        println!("{fault}");
    }
    println!("\n== unresolved, most call sites first");
    let mut rows: Vec<_> = census.unresolved.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (fault, sites) in rows {
        println!("{sites:5}  {fault}");
    }
}
