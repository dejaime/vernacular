use criterion::{criterion_group, criterion_main, Criterion, black_box};
use std::fs;
use vernacular::VernacularContext;

fn generate_massive_csv(dir: &std::path::Path) {
    let mut content = String::from("key,en_US,ja_JP,es_ES\n");
    for i in 0..10000 {
        content.push_str(&format!(
            "ui.key_{i},English {i},Japanese {i},Spanish {i}\n"
        ));
    }
    fs::write(dir.join("massive.csv"), content).unwrap();
}

pub fn bench_cold_load(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    generate_massive_csv(temp_dir.path());

    c.bench_function("cold load and parse 10k csv", |b| {
        b.iter(|| {
            let ctx = VernacularContext::new();
            ctx.set_content_path(temp_dir.path().to_str().unwrap());
            ctx.set_locale("en_US");
            // First localize triggers the full cold load
            black_box(ctx.localize("ui.key_5000"));
        })
    });
}

pub fn bench_hot_lookup(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    generate_massive_csv(temp_dir.path());

    let ctx = VernacularContext::new();
    ctx.set_content_path(temp_dir.path().to_str().unwrap());
    ctx.set_locale("en_US");
    // Ensure it's fully loaded
    black_box(ctx.localize("ui.key_5000"));

    c.bench_function("hot lookup", |b| {
        b.iter(|| {
            black_box(ctx.localize("ui.key_5000"));
        })
    });
}

pub fn bench_template_format(c: &mut Criterion) {
    let temp_dir = tempfile::tempdir().unwrap();
    fs::write(
        temp_dir.path().join("template.csv"),
        "key,en_US\nui.msg,Hello {0}! You have {} new messages and {} notifications.\n"
    ).unwrap();

    let ctx = VernacularContext::new();
    ctx.set_content_path(temp_dir.path().to_str().unwrap());
    ctx.set_locale("en_US");
    black_box(ctx.localize_fmt("ui.msg", &[&"Alice", &"5", &"10"]));

    c.bench_function("template format", |b| {
        b.iter(|| {
            black_box(ctx.localize_fmt("ui.msg", &[&"Alice", &"5", &"10"]));
        })
    });
}

criterion_group!(benches, bench_cold_load, bench_hot_lookup, bench_template_format);
criterion_main!(benches);
