# Marketplace Integration Boundary

## Purpose

Vision Desktop now includes a read-only Marketplace view derived from the legacy wallet-marketplace visual hierarchy. The view establishes a safe presentation boundary without importing the legacy runtime or claiming that current Vision Core exposes marketplace behavior.

The legacy reference was inspected at:

`C:\Vision\vision-node\vision node\wallet-marketplace-source`

The legacy repository remains reference material only. It was not modified.

## Current Desktop Slice

The Marketplace view currently provides:

- a market-terminal layout with explicit empty and unavailable states;
- Core process, API, recovery, mock-mode, and Desktop refresh context from existing typed Desktop state;
- a feature map for exchange market data, land listings, and cash-order operations;
- a visible statement that market data and financial actions are not connected;
- pure status derivation with executable tests;
- navigation through the existing `DesktopView` and `ActiveViewChanged` event path.

No new state store, polling loop, service wrapper, Tauri command, Rust backend command, dependency, or Vision Core API was added.

## Legacy Patterns Not Reused

The legacy source contains useful visual structures, but its runtime patterns are not compatible with the current Desktop boundary. Vision Desktop deliberately did not reuse:

- direct browser `fetch` calls to marketplace and exchange endpoints;
- hard-coded `localhost:7070` service access;
- Vite development proxies as a production API boundary;
- independent 2-second, 3-second, 5-second, 10-second, and 15-second polling loops;
- direct WebSocket market streams;
- React Router navigation and Zustand wallet coupling;
- browser-window checkout and QR workflows;
- clipboard access and client-side document writing;
- floating-point conversion, multiplication, and display assumptions for blockchain or market amounts;
- `any`-typed order status and API response mapping;
- buy, sell, order placement, webhook simulation, mint replay, or other write actions;
- wallet custody, key, signing, deposit, or ownership behavior.

These patterns would bypass or duplicate the current `coreApi.ts`, Desktop state, typed event, reducer, and request-ordering boundaries.

## Current Data Truth

Vision Desktop does not currently have an approved typed source for:

- trading pairs;
- prices, volumes, tickers, or charts;
- bids, asks, or recent trades;
- balances available for trading;
- open or historical orders;
- land listings or settlement status;
- cash orders, payments, checkouts, webhooks, or mint status.

The Marketplace view therefore displays no mock numerical market data and makes no availability, ownership, settlement, or trading claim.

## Required Future Decision

Live read-only marketplace data should not be added until the authoritative service is selected and its compatibility boundary is explicitly approved. That decision must establish:

- whether the data is owned by Vision Core or a separate marketplace service;
- a loopback-safe, authenticated, typed Desktop backend boundary;
- exact string or integer representations for prices and amounts, including denomination and precision metadata;
- supported identifiers, pairs, status values, pagination, freshness, and error semantics;
- privacy and redaction requirements for addresses, orders, payments, and support packages;
- request ordering, rate limits, caching, and event or polling behavior;
- a read-only first phase before any transaction-capable work.

Any write-capable phase requires a separate approved design for custody, authorization, signing, transaction submission, confirmations, payment handling, and financial-action testing.

## Security Position

The current slice does not modify Vision Core, contact the legacy services, open external checkout windows, transmit data, or create financial actions. It preserves the existing Vision Desktop service, state, event, reducer, polling, and request-ordering architecture.
