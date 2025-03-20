use reqwest::Client;
use rusqlite::{params, Connection};
use serde::Deserialize;
use chrono::{Utc, DateTime};
use clap::{Parser, ValueEnum};
use std::env;

const BASE_URL: &str = "https://api-fxtrade.oanda.com/v3";
const MAX_CANDLES: usize = 2000;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Database name
    #[arg(short, long, default_value = "oanda.db")]
    db: String,

    /// Granularity (D, W, M), defaults to all if not provided
    #[arg(short, long, value_enum, num_args = 0.., default_values = ["D", "W", "M"], ignore_case = true)]
    granularity: Vec<Granularity>,

    /// OANDA Account ID (overrides env variable)
    #[arg(long)]
    oanda_account_id: Option<String>,

    /// OANDA Access Token (overrides env variable)
    #[arg(long)]
    oanda_access_token: Option<String>,

    /// Comma-separated list of tickers (whitelist), e.g., --tickers natgas_usd,xau_usd,eur_usd,spx500_usd
    #[arg(long)]
    tickers: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
enum Granularity {
    D,
    W,
    M,
}

#[derive(Debug, Deserialize)]
struct OandaInstruments {
    instruments: Vec<Instrument>,
}

#[derive(Debug, Deserialize)]
struct Instrument {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CandleResponse {
    candles: Vec<Candle>,
}

#[derive(Debug, Deserialize)]
struct Candle {
    time: DateTime<Utc>,
    complete: bool,
    volume: f64,
    mid: OHLC,
}

#[derive(Debug, Deserialize)]
struct OHLC {
    o: String,
    h: String,
    l: String,
    c: String,
}

async fn fetch_instruments(client: &Client, token: &str, account_id: &str) -> reqwest::Result<Vec<String>> {
    let url = format!("{}/accounts/{}/instruments", BASE_URL, account_id);
    let res: OandaInstruments = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await?
        .json()
        .await?;

    Ok(res.instruments.into_iter().map(|i| i.name).collect())
}

async fn fetch_candles(client: &Client, token: &str, instrument: &str, granularity: &str, from: Option<DateTime<Utc>>) -> reqwest::Result<CandleResponse> {
    let mut req = client
        .get(format!("{}/instruments/{}/candles", BASE_URL, instrument))
        .bearer_auth(token)
        .query(&[("price", "M"), ("granularity", granularity), ("count", "500")]);

    if let Some(from_time) = from {
        req = req.query(&[("from", from_time.timestamp().to_string())]);
    }

    req.send().await?.json().await
}

fn setup_table(conn: &Connection, table: &str) {
    conn.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {} (
                timestamp INTEGER,
                open REAL,
                high REAL,
                low REAL,
                close REAL,
                volume REAL
            );", table),
        [],
    ).unwrap();
}

fn insert_candles(conn: &mut Connection, table: &str, candles: &[Candle]) {
    {
        let tx = conn.transaction().unwrap();

        for candle in candles {
            if candle.complete {
                tx.execute(
                    &format!(
                        "INSERT INTO {} (timestamp, open, high, low, close, volume) VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
                        table
                    ),
                    params![
                        candle.time.timestamp(),
                        candle.mid.o.parse::<f64>().unwrap(),
                        candle.mid.h.parse::<f64>().unwrap(),
                        candle.mid.l.parse::<f64>().unwrap(),
                        candle.mid.c.parse::<f64>().unwrap(),
                        candle.volume
                    ],
                ).unwrap();
            }
        }

        tx.execute(
            &format!(
                "DELETE FROM {} WHERE rowid IN (SELECT rowid FROM {} ORDER BY timestamp DESC LIMIT -1 OFFSET ?1);",
                table, table
            ),
            [MAX_CANDLES],
        ).unwrap();

        tx.commit().unwrap();
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let token = args.oanda_access_token
        .unwrap_or_else(|| env::var("OANDA_ACCESS_TOKEN").expect("OANDA_ACCESS_TOKEN not set"));
    let account_id = args.oanda_account_id
        .unwrap_or_else(|| env::var("OANDA_ACCOUNT_ID").expect("OANDA_ACCOUNT_ID not set"));

    let db_path = args.db;
    
    // Create whitelist from the --tickers argument if provided, otherwise use the default list.
    let whitelist: Vec<String> = if let Some(tickers) = args.tickers {
        tickers.split(',')
            .map(|s| s.trim().to_lowercase())
            .collect()
    } else {
        vec![
            "natgas_usd".to_string(), "xau_usd".to_string(), "eur_usd".to_string(),
            "de30_eur".to_string(), "xcu_usd".to_string(), "xag_usd".to_string(),
            "xau_usd".to_string(), "sugar_usd".to_string(), "wtico_usd".to_string(),
            "wheat_usd".to_string(), "corn_usd".to_string(), "spx500_usd".to_string(),
            "jp225_usd".to_string(), "cn50_usd".to_string(), "eu50_eur".to_string(),
            "fr40_eur".to_string(), "xau_xag".to_string()
        ]
    };

    let client = Client::new();
    let all_instruments = fetch_instruments(&client, &token, &account_id).await.unwrap();
    let selected_granularities: Vec<String> = args.granularity.iter().map(|g| format!("{:?}", g)).collect();

    let mut conn = Connection::open(db_path).unwrap();

    for instrument in all_instruments.iter().filter(|inst| whitelist.iter().any(|w| inst.to_lowercase().starts_with(w))) {
        for granularity in &selected_granularities {
            let table_name = format!("{}_{}", instrument.to_lowercase(), granularity);
            setup_table(&conn, &table_name);

            let last_timestamp: Option<DateTime<Utc>> = conn.query_row(
                &format!("SELECT timestamp FROM {} ORDER BY timestamp DESC LIMIT 1", table_name),
                [],
                |row| row.get::<_, i64>(0).map(|ts| DateTime::from_timestamp(ts, 0).unwrap()),
            ).ok();

            let candles_resp = fetch_candles(&client, &token, &instrument, granularity, last_timestamp).await.unwrap();

            println!("Fetched {} candles for {}", candles_resp.candles.len(), table_name);

            insert_candles(&mut conn, &table_name, &candles_resp.candles);
        }
    }

    println!("Sync complete!");
}
