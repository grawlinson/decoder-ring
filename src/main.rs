//! `decoder-ring` — command-line port of Ambrosia Software's internal
//! DecoderRing.app (v1.0.0, 2011) licence-code generator.

mod generator;
mod products;
mod rt3;

use chrono::{Local, TimeZone};
use clap::{Parser, Subcommand, ValueEnum};
use products::{Product, Scheme};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "decoder-ring", version, about = "Ambrosia DecoderRing licence-code generator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemeArg {
    Old1,
    Old2,
    Rt3,
    Rt3Retail,
}

#[derive(clap::Args)]
struct Target {
    /// Product as shown by `list` (case/punctuation-insensitive, unique prefix OK)
    #[arg(short, long, required_unless_present = "custom")]
    product: Option<String>,
    /// Use an arbitrary product key instead of the built-in menu
    #[arg(long, conflicts_with = "product", requires = "scheme")]
    custom: Option<String>,
    /// Scheme for --custom
    #[arg(long, value_enum)]
    scheme: Option<SchemeArg>,
    /// Sub-item depth for --custom (algorithm variant)
    #[arg(long, default_value_t = 0)]
    depth: i32,
    /// Licensee name (or a 6-digit retail ID for `**` products)
    #[arg(short, long)]
    name: String,
    /// Number of copies
    #[arg(short, long, default_value_t = 1)]
    copies: u16,
}

#[derive(Subcommand)]
enum Cmd {
    /// List the products known to DecoderRing
    List,
    /// Generate a licence code
    Generate {
        #[command(flatten)]
        target: Target,
        /// Pretend the current time is this UNIX timestamp (RT3 codes embed a 2-week period)
        #[arg(long)]
        now: Option<i64>,
    },
    /// Check an RT3 licence code against a product/name/copies
    Verify {
        #[command(flatten)]
        target: Target,
        /// The licence code (XXXX-XXXX-XXXX)
        #[arg(short = 'k', long)]
        code: String,
    },
}

fn resolve(t: &Target) -> Result<Product, String> {
    if let Some(key) = &t.custom {
        let scheme = match t.scheme.unwrap() {
            SchemeArg::Old1 => Scheme::OldStyle1,
            SchemeArg::Old2 => Scheme::OldStyle2,
            SchemeArg::Rt3 => Scheme::Rt3 { retail: false },
            SchemeArg::Rt3Retail => Scheme::Rt3 { retail: true },
        };
        return Ok(Product { display: key.clone(), key: key.clone(), depth: t.depth, scheme });
    }
    products::find(t.product.as_deref().unwrap())
}

/// `-[ASWRegDecoderRingWC cStringFromString:]` uses NSMacOSRomanStringEncoding.
fn mac_roman(s: &str, what: &str) -> Result<Vec<u8>, String> {
    let (bytes, _, unmappable) = encoding_rs::MACINTOSH.encode(s);
    if unmappable {
        return Err(format!("{what} contains characters not representable in Mac OS Roman"));
    }
    Ok(bytes.into_owned())
}

fn ctime(t: i64) -> String {
    Local
        .timestamp_opt(t, 0)
        .single()
        .map(|d| d.format("%a %b %e %H:%M:%S %Y").to_string())
        .unwrap_or_else(|| "?".into())
}

fn cmd_list() {
    println!("{:<36} {:<20} {}", "PRODUCT", "SCHEME", "KEY/DEPTH");
    for p in products::all() {
        println!("{:<36} {:<20} {:?}/{}", p.display, p.scheme.label(), p.key, p.depth);
    }
}

fn cmd_generate(t: &Target, now: Option<i64>) -> Result<(), String> {
    if t.copies == 0 {
        return Err("copies must be at least 1".into());
    }
    let p = resolve(t)?;
    let name = mac_roman(&t.name, "name")?;
    let key = mac_roman(&p.key, "product")?;
    if name.is_empty() {
        return Err("name must not be empty".into());
    }

    println!("Product:   {}", p.display);
    println!("Licensee:  {}", t.name);
    println!("Copies:    {}", t.copies);

    match p.scheme {
        Scheme::External => {
            println!("Code:      (none — {} is licensed externally; DecoderRing emits no code)", p.display);
        }
        Scheme::OldStyle1 | Scheme::OldStyle2 => {
            let style = if p.scheme == Scheme::OldStyle1 { 1 } else { 2 };
            let code = generator::generate_old_style_code(&key, &name, t.copies, style);
            println!("Code:      {code}");
        }
        Scheme::Rt3 { retail } => {
            let alg = generator::generate_algorithm(&key, p.depth);
            if retail {
                if let Some(d) = rt3::retail_digits(&name) {
                    let id: u32 = std::str::from_utf8(&d).unwrap().parse().unwrap();
                    let code = generator::generate_retail_code(&alg, &key, id);
                    println!("Code:      {}", rt3::code_to_text(code));
                    println!("Generated: Retail code");
                    println!("Expires:   Never");
                    return Ok(());
                }
            }
            let now = now.unwrap_or_else(|| Local::now().timestamp());
            // Original stores time() in a 32-bit int.
            let now32 = now as i32 as i64;
            let period = rt3::time_period((now32 - 86_400) as u32);
            let code = generator::generate_license_code(&alg, &key, &name, t.copies as i16 as i32, period);
            println!("Code:      {}", rt3::code_to_text(code));
            println!("Generated: {}", ctime(now32));
            println!("Expires:   {}", ctime(rt3::period_expiry(period) as i64));
        }
    }
    Ok(())
}

fn cmd_verify(t: &Target, code: &str) -> Result<bool, String> {
    let p = resolve(t)?;
    let Scheme::Rt3 { retail } = p.scheme else {
        // Old-style codes are a pure function of the inputs: regenerate and compare.
        if p.scheme == Scheme::External {
            return Err("externally licensed products have no DecoderRing codes".into());
        }
        let style = if p.scheme == Scheme::OldStyle1 { 1 } else { 2 };
        let expect = generator::generate_old_style_code(
            &mac_roman(&p.key, "product")?,
            &mac_roman(&t.name, "name")?,
            t.copies,
            style,
        );
        let ok = expect.eq_ignore_ascii_case(code.trim());
        println!("{}", if ok { "VALID" } else { "INVALID" });
        return Ok(ok);
    };
    let key = mac_roman(&p.key, "product")?;
    let name = mac_roman(&t.name, "name")?;
    let alg = generator::generate_algorithm(&key, p.depth);
    let value = rt3::text_to_code(code);
    if retail {
        if let Some(d) = rt3::retail_digits(&name) {
            let id: u32 = std::str::from_utf8(&d).unwrap().parse().unwrap();
            let ok = generator::validate_retail_code(&alg, &key, id, value);
            println!("{}", if ok { "VALID (retail)" } else { "INVALID" });
            return Ok(ok);
        }
    }
    match generator::validate_license_code(&alg, &key, &name, t.copies as i16 as i32, value) {
        Some(period) => {
            println!("VALID — time period {period}, expires {}", ctime(rt3::period_expiry(period) as i64));
            Ok(true)
        }
        None => {
            println!("INVALID");
            Ok(false)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let res = match &cli.cmd {
        Cmd::List => {
            cmd_list();
            Ok(true)
        }
        Cmd::Generate { target, now } => cmd_generate(target, *now).map(|_| true),
        Cmd::Verify { target, code } => cmd_verify(target, code),
    };
    match res {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
