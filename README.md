# oandaohlc

A tiny program which connects to Oanda API and downloads OHLC candle data, normalizes it and dumps it into a sqlite database.

## Build

> cargo build --release

### Cross build

> cross build --release --target arm-unknown-linux-gnueabihf

## Setup

Export OANDA_ACCOUNT_ID and OANDA_ACCESS_TOKEN to env.

## Run

```
Usage: oandaohlc [OPTIONS]

Options:
  -d, --db <DB>
          Database name [default: oanda.db]
  -g, --granularity [<GRANULARITY>...]
          Granularity (D, W, M), defaults to all if not provided [default: D W M] [possible values: d, w, m]
      --oanda-account-id <OANDA_ACCOUNT_ID>
          OANDA Account ID (overrides env variable)
      --oanda-access-token <OANDA_ACCESS_TOKEN>
          OANDA Access Token (overrides env variable)
      --tickers <TICKERS>
          Comma-separated list of tickers (whitelist), e.g., --tickers natgas_usd,xau_usd,eur_usd
  -h, --help
          Print help
  -V, --version
          Print version
```
