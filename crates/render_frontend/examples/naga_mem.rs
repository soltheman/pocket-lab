//! How much memory parsed shader modules hold: parses and validates the WGSL cache's
//! modules with naga (as wgpu does before keeping them for per-pipeline compiles) and
//! reports the heap the kept `Module` + `ModuleInfo` pairs occupy.
//!
//! `cargo run --profile play -p render_frontend --example naga_mem -- <wgsl cache dir> [count]`
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            LIVE.fetch_add(new_size, Ordering::Relaxed);
        }
        new
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const HEADER: usize = 8 + 24;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("wgsl cache dir"));
    let count: usize = args.next().map_or(500, |n| n.parse().expect("count"));
    let mut files = Vec::new();
    for shard in std::fs::read_dir(&dir).expect("read cache dir") {
        let shard = shard.unwrap().path();
        if shard.is_dir() {
            for file in std::fs::read_dir(shard).unwrap() {
                files.push(file.unwrap().path());
            }
        }
    }
    files.sort();
    files.truncate(count);

    let mut kept = Vec::with_capacity(files.len());
    let (mut source_bytes, mut expressions, mut failed) = (0usize, 0usize, 0usize);
    let before = LIVE.load(Ordering::Relaxed);
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        // The header has grown between cache formats; the WGSL starts at its first struct.
        let start = bytes
            .windows(7)
            .position(|w| w == b"struct ")
            .unwrap_or(HEADER);
        let Some(source) = bytes.get(start..).and_then(|s| std::str::from_utf8(s).ok()) else {
            failed += 1;
            continue;
        };
        source_bytes += source.len();
        let module = match naga::front::wgsl::parse_str(source) {
            Ok(module) => module,
            Err(error) => {
                if failed == 0 {
                    eprintln!("{}: {}", path.display(), error.emit_to_string(source));
                }
                failed += 1;
                continue;
            }
        };
        let Ok(info) = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module) else {
            failed += 1;
            continue;
        };
        expressions += module
            .entry_points
            .iter()
            .map(|ep| ep.function.expressions.len())
            .chain(module.functions.iter().map(|(_, f)| f.expressions.len()))
            .sum::<usize>();
        kept.push((module, info));
    }
    let held = LIVE.load(Ordering::Relaxed) - before;
    let n = kept.len().max(1);
    println!(
        "{} modules kept ({failed} failed): {:.1} MB WGSL source -> {:.1} MB held as naga Module+ModuleInfo ({:.0} KB per module, {:.1}x the source), {} expressions per module",
        kept.len(),
        source_bytes as f64 / 1048576.0,
        held as f64 / 1048576.0,
        held as f64 / 1024.0 / n as f64,
        held as f64 / source_bytes.max(1) as f64,
        expressions / n
    );
    // Without the info, which a backend could rebuild on demand.
    let infos: usize = {
        let before = LIVE.load(Ordering::Relaxed);
        let modules: Vec<_> = kept.into_iter().map(|(module, _)| module).collect();
        let after = LIVE.load(Ordering::Relaxed);
        drop(modules);
        before - after
    };
    println!("of which ModuleInfo: {:.1} MB", infos as f64 / 1048576.0);
}
