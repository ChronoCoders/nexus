use criterion::{Criterion, black_box, criterion_group, criterion_main};
use nexus_fix_codec::{FixDate, FixDecimal, FixTime, FixTimestamp, parse_fix_int, parse_fix_seqnum, parse_fix_uint};

fn bench_fix_decimal(c: &mut Criterion) {
    let mut g = c.benchmark_group("FixDecimal::parse");

    g.bench_function("4_digit_price", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"99.50")))
    });

    g.bench_function("8_digit_price", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"50123.450")))
    });

    g.bench_function("12_digit_price", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"50123.45000000")))
    });

    g.bench_function("16_digit_price", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"1234567.890123456")))
    });

    g.bench_function("integer_only", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"12345678")))
    });

    g.bench_function("negative", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"-123.456")))
    });

    g.bench_function("sub_penny", |b| {
        b.iter(|| FixDecimal::parse(black_box(b"0.00000001")))
    });

    g.finish();
}

fn bench_fix_int(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_fix_int");

    g.bench_function("1_digit", |b| {
        b.iter(|| parse_fix_int(black_box(b"7")))
    });

    g.bench_function("4_digit", |b| {
        b.iter(|| parse_fix_int(black_box(b"1234")))
    });

    g.bench_function("8_digit", |b| {
        b.iter(|| parse_fix_int(black_box(b"12345678")))
    });

    g.bench_function("16_digit", |b| {
        b.iter(|| parse_fix_int(black_box(b"1234567890123456")))
    });

    g.bench_function("19_digit_max", |b| {
        b.iter(|| parse_fix_int(black_box(b"9223372036854775807")))
    });

    g.bench_function("negative_8", |b| {
        b.iter(|| parse_fix_int(black_box(b"-12345678")))
    });

    g.finish();
}

fn bench_fix_uint(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_fix_uint");

    g.bench_function("body_length", |b| {
        b.iter(|| parse_fix_uint(black_box(b"256")))
    });

    g.bench_function("num_in_group", |b| {
        b.iter(|| parse_fix_uint(black_box(b"12")))
    });

    g.finish();
}

fn bench_fix_seqnum(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_fix_seqnum");

    g.bench_function("small", |b| {
        b.iter(|| parse_fix_seqnum(black_box(b"1000")))
    });

    g.bench_function("typical", |b| {
        b.iter(|| parse_fix_seqnum(black_box(b"1000000")))
    });

    g.bench_function("large", |b| {
        b.iter(|| parse_fix_seqnum(black_box(b"99999999999")))
    });

    g.finish();
}

fn bench_fix_timestamp(c: &mut Criterion) {
    let mut g = c.benchmark_group("FixTimestamp::parse");

    g.bench_function("no_frac", |b| {
        b.iter(|| FixTimestamp::parse(black_box(b"20260602-14:30:00")))
    });

    g.bench_function("millis", |b| {
        b.iter(|| FixTimestamp::parse(black_box(b"20260602-14:30:00.123")))
    });

    g.bench_function("micros", |b| {
        b.iter(|| FixTimestamp::parse(black_box(b"20260602-14:30:00.123456")))
    });

    g.bench_function("nanos", |b| {
        b.iter(|| FixTimestamp::parse(black_box(b"20260602-14:30:00.123456789")))
    });

    g.finish();
}

fn bench_fix_date(c: &mut Criterion) {
    c.bench_function("FixDate::parse", |b| {
        b.iter(|| FixDate::parse(black_box(b"20260602")))
    });
}

fn bench_fix_time(c: &mut Criterion) {
    let mut g = c.benchmark_group("FixTime::parse");

    g.bench_function("no_frac", |b| {
        b.iter(|| FixTime::parse(black_box(b"14:30:00")))
    });

    g.bench_function("micros", |b| {
        b.iter(|| FixTime::parse(black_box(b"14:30:00.123456")))
    });

    g.finish();
}

criterion_group!(
    benches,
    bench_fix_decimal,
    bench_fix_int,
    bench_fix_uint,
    bench_fix_seqnum,
    bench_fix_timestamp,
    bench_fix_date,
    bench_fix_time,
);
criterion_main!(benches);
