# Open Payments Workshop Rust

This folder contains a Rust rewrite of the workshop script from the root `index.js` file.

## What it does

- Loads the client wallet address, sender wallet address, and receiver wallet address
- Requests an incoming payment grant and creates one incoming payment
- Requests an interactive outgoing payment grant
- Waits for you to complete the browser interaction, then continues the grant
- Creates two outgoing payments from the incoming payment

## Environment variables

Copy `.env.example` to `.env` and fill in the values:

- `CLIENT_WALLET_ADDRESS_URL`
- `SENDING_WALLET_ADDRESS_URL`
- `RECEIVING_WALLET_ADDRESS_URL`
- `KEY_ID`
- `PRIVATE_KEY_PATH`
- `INTERACT_FINISH_URI` optional, defaults to `http://localhost/callback`

## Run

```sh
cargo run
```

If the outgoing grant requires interaction, open the printed redirect URL in your browser, approve the grant, then paste the callback URL that contains `interact_ref`.
