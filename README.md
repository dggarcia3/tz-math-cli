# tzmath

Offset arithmetic for civil timestamps, without a timezone database.

Most timestamp bugs I run into aren't about DST rules or IANA zone names,
they're about getting the basic offset math wrong: turning a wall-clock
reading plus a UTC offset into an absolute instant, and back again under a
different offset, without losing or gaining a day at a boundary. This is a
small, dependency-free implementation of that core: a validating parser for
`YYYY-MM-DDTHH:MM:SS[.fraction]±HH:MM` timestamps, a pretty printer that
normalizes them back to a canonical form, and the conversion/arithmetic in
between. Fractional seconds are optional, parsed to nanosecond precision,
and printed with trailing zeros trimmed.

There is no DST support and no named timezones ("America/New_York" is out
of scope) - only fixed numeric offsets, which is what the wire format for
most APIs and log files actually carries.

## Usage

As a library:

```rust
use tzmath::{DateTime, Offset};

let dt = DateTime::parse("2024-03-10T14:30:00-05:00")?;

// Same instant, read under a different offset.
let tokyo = dt.with_offset(Offset::parse("+09:00")?);
println!("{tokyo}"); // 2024-03-11T04:30:00+09:00

// Arithmetic that correctly rolls over month/year boundaries.
let later = dt.add_seconds(3600 * 6);
println!("{later}"); // 2024-03-10T20:30:00-05:00
```

As a CLI:

```
$ cargo run -- convert 2024-03-10T14:30:00-05:00 +09:00
2024-03-11T04:30:00+09:00

$ cargo run -- add 2024-03-10T14:30:00-05:00 3h30m
2024-03-10T18:00:00-05:00

$ cargo run -- convert 2024-02-29T23:00:00Z -05:00
2024-02-29T18:00:00-05:00

$ cargo run -- convert 2024-02-30T00:00:00Z +00:00
error: day 30 out of range for 2024-02
```

Invalid input is rejected with a specific reason (bad separators, an
out-of-range field, an offset outside -12:00..=+14:00), not a panic.

## How the conversion works

Dates are converted to a day count via Howard Hinnant's `days_from_civil`
algorithm (and back via its inverse, `civil_from_days`), which is exact
for the proleptic Gregorian calendar across the full range of `i32` years,
including leap years computed by the usual 4/100/400 rule. Combined with
the offset in minutes, that gives an absolute Unix-epoch second count that
all the conversion and arithmetic is built on.

## License

MIT, see [LICENSE](LICENSE).
