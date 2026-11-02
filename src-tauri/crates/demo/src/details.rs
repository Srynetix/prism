//! What the window reads on demand in the demo profile: an MR's description, threads and diff,
//! a pipeline's jobs and their logs, a ticket's content, a pod's log, an agenda event, an
//! agent's folder. Built from the demo snapshot, so that every link leads somewhere.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use super::{job_id, Demo, HELM, HOST, PROJECT, STAGE_JOBS};
use crate::{
    agents::files::{Diff, FileEntry, Found, Listing, Match, Preview, Query},
    collectors::{
        calendar::{Attendee, EventDetails},
        environments::{PodInfo, PodLog},
        task_view::{Checklist, ChecklistItem, Comment, Subtask, TaskView},
    },
    forge::{approval_history, ApprovalEvent, ApprovalKind, Discussion, FileDiff, ForgeLabel, JobInfo, JobLog, Member, MrCommit, MrDetail, MrDiff, Note},
    state::{CalendarEvent, HistorySource, Snapshot, Ticket, UsageHistory, UsagePoint},
};

fn now() -> DateTime<Utc> {
    super::LAUNCH.get().copied().unwrap_or_else(Utc::now)
}

fn ago(minutes: i64) -> DateTime<Utc> {
    now() - Duration::minutes(minutes)
}

/// The team's GitLab login and name: `alex` for Alex Martin.
fn login(i: usize) -> (String, String) {
    let p = &Demo::team()[i];
    (p.name.split_whitespace().next().unwrap_or_default().to_lowercase(), p.name.clone())
}

/* ---------- Diffs ---------- */

/// A hunk from its lines, each led by `+`, `-` or `| ` (unchanged); its header counts them.
/// Leading spaces before the marker are the source's indentation, not the file's.
fn hunk(old: u32, new: u32, context: &str, body: &str) -> String {
    let lines: Vec<String> = body
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_at(1) {
            ("|", rest) => format!(" {}", rest.strip_prefix(' ').unwrap_or(rest)),
            (m, rest) => format!("{m}{rest}"),
        })
        .collect();
    let olds = lines.iter().filter(|l| !l.starts_with('+')).count();
    let news = lines.iter().filter(|l| !l.starts_with('-')).count();
    format!("@@ -{old},{olds} +{new},{news} @@ {context}\n{}\n", lines.join("\n"))
}

fn changed(path: &str, hunks: &[String]) -> FileDiff {
    FileDiff { old_path: path.into(), new_path: path.into(), new_file: false, deleted_file: false, renamed_file: false, diff: hunks.concat(), collapsed: false }
}

fn added(path: &str) -> FileDiff {
    let text = file(path).unwrap_or_default();
    let lines: Vec<String> = text.lines().map(|l| format!("+{l}")).collect();
    FileDiff {
        old_path: path.into(),
        new_path: path.into(),
        new_file: true,
        deleted_file: false,
        renamed_file: false,
        diff: format!("@@ -0,0 +1,{} @@\n{}\n", lines.len(), lines.join("\n")),
        collapsed: false,
    }
}

/// The files the demo knows whole: a new file's diff, the Markdown preview, an agent's folder.
fn file(path: &str) -> Option<&'static str> {
    Some(match path {
        "services/orders-api/src/cart/cart.repository.ts" => CART_REPOSITORY,
        "services/storefront-webapp/src/cart/useCart.ts" => USE_CART,
        "services/orders-api/migrations/2026_10_06_cart_lines.sql" => CART_MIGRATION,
        "services/orders-api/src/invoices/vat.ts" => VAT,
        "services/orders-api/src/routes.ts" => ROUTES,
        "services/catalog-api/src/search.ts" => SEARCH,
        "services/payments-api/webhooks/retry.go" => RETRY,
        "services/storefront-webapp/src/catalog/ProductCard.tsx" => PRODUCT_CARD,
        "services/catalog-api/package.json" => CATALOG_PACKAGE,
        "services/orders-api/src/invoices/vat.test.ts" => VAT_TEST,
        "services/storefront-webapp/src/account/OrderHistory.tsx" => ORDER_HISTORY,
        "services/orders-api/src/orders/orders.repository.ts" => ORDERS_REPOSITORY,
        "services/storefront-webapp/src/catalog/ProductImage.tsx" => PRODUCT_IMAGE,
        "services/backoffice/src/orders/export.ts" => CSV_EXPORT,
        "docs/refinement/SHOP-441-multi-currency.md" => MULTI_CURRENCY,
        "CHANGELOG.md" => CHANGELOG,
        "README.md" => README,
        _ => return None,
    })
}

/// Changed by !1288, its new version: the lines around its hunks, shown on demand.
const USE_CART: &str = r#"import { useEffect, useState } from 'react'
import { api } from '../api'
import { useSession } from '../account/useSession'
import type { Cart } from './types'
import { emptyCart, mergeCarts, readLocalCart } from './cart'

/**
 * The cart shown in the header and at checkout: the account's once signed in (kept by
 * orders-api), the browser's for a guest, merged into the account's at sign-in.
 */
export function useCart() {
  // Empty until read: the header shows no count rather than a stale one.

  const [cart, setCart] = useState<Cart>(emptyCart)
  const { customer } = useSession()

  // Signed in: the cart is the account's, kept by orders-api between sessions.
  useEffect(() => {
    if (customer) api.get<Cart>('/cart').then((saved) => setCart(mergeCarts(saved, readLocalCart())))
    else setCart(readLocalCart())
  }, [customer])

  const update = (next: Cart) => {
    setCart(next)
    if (customer) void api.put('/cart', next)
    else localStorage.setItem('cart', JSON.stringify(next))
  }

  const add = (sku: string, quantity = 1) => {
    const line = cart.lines.find((l) => l.sku === sku)
    update({
      ...cart,
      lines: line
        ? cart.lines.map((l) => (l.sku === sku ? { ...l, quantity: l.quantity + quantity } : l))
        : [...cart.lines, { sku, quantity }],
    })
  }

  const remove = (sku: string) => update({ ...cart, lines: cart.lines.filter((l) => l.sku !== sku) })

  const setQuantity = (sku: string, quantity: number) =>
    quantity <= 0 ? remove(sku) : update({ ...cart, lines: cart.lines.map((l) => (l.sku === sku ? { ...l, quantity } : l)) })

  const count = cart.lines.reduce((n, l) => n + l.quantity, 0)

  return { cart, count, add, remove, setQuantity }
}

/** The quantity of a product in the cart, 0 if absent: the product card's badge. */
export function quantityOf(cart: Cart, sku: string): number {
  return cart.lines.find((l) => l.sku === sku)?.quantity ?? 0
}
"#;

const CART_REPOSITORY: &str = r#"import { sql } from '../db'
import type { Cart, CartLine } from './cart.types'

/** Carts of signed-in customers, kept between sessions and devices. */
export class CartRepository {
  async find(customerId: string): Promise<Cart | null> {
    const lines = await sql<CartLine[]>`
      SELECT sku, quantity, unit_price AS "unitPrice"
      FROM cart_lines
      WHERE customer_id = ${customerId}
      ORDER BY added_at`
    return lines.length ? { customerId, lines } : null
  }

  async save(cart: Cart): Promise<void> {
    await sql.begin(async (tx) => {
      await tx`DELETE FROM cart_lines WHERE customer_id = ${cart.customerId}`
      for (const line of cart.lines) {
        await tx`
          INSERT INTO cart_lines (customer_id, sku, quantity, unit_price)
          VALUES (${cart.customerId}, ${line.sku}, ${line.quantity}, ${line.unitPrice})`
      }
    })
  }
}
"#;

const CART_MIGRATION: &str = r#"CREATE TABLE cart_lines (
  customer_id uuid        NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
  sku         text        NOT NULL,
  quantity    integer     NOT NULL CHECK (quantity > 0),
  unit_price  numeric(10, 2) NOT NULL,
  added_at    timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (customer_id, sku)
);
"#;

/// Changed by !1290, its new version: the customer's orders a page at a time.
const ORDERS_REPOSITORY: &str = r#"import { sql } from '../db'
import type { Order } from './types'

/** Where a page of orders stops: the last one's creation time and id (two can share a time). */
export interface Cursor {
  createdAt: string
  id: string
}

export interface Page<T> {
  items: T[]
  next: Cursor | null
}

const cursorOf = (o: Order): Cursor => ({ createdAt: o.created_at, id: o.id })

export class OrdersRepository {
  async get(id: string): Promise<Order | undefined> {
    const [order] = await sql<Order[]>`SELECT * FROM orders WHERE id = ${id}`
    return order
  }

  // A customer's orders, for the account page and the support tools.
  /** A page of a customer's orders, newest first; `after`: the last order of the previous page. */
  async listByCustomer(customerId: string, after?: Cursor, limit = 20): Promise<Page<Order>> {
    const rows = await sql<Order[]>`
      SELECT * FROM orders
      WHERE customer_id = ${customerId}
        AND (${after ?? null}::text IS NULL OR (created_at, id) < (${after?.createdAt}, ${after?.id}))
      ORDER BY created_at DESC, id DESC
      LIMIT ${limit + 1}`
    return { items: rows.slice(0, limit), next: rows.length > limit ? cursorOf(rows[limit - 1]) : null }
  }

  async create(order: Omit<Order, 'id' | 'created_at'>): Promise<Order> {
    const [created] = await sql<Order[]>`INSERT INTO orders ${sql(order)} RETURNING *`
    return created
  }
}
"#;

/// Changed by !1290, its new version: the history loads 20 orders at a time.
const ORDER_HISTORY: &str = r#"import { useInfiniteQuery } from '@tanstack/react-query'
import { api } from '../api'
import { Button, Spinner } from '../ui'
import type { Order, Page } from '../types'
import { OrderRow } from './OrderRow'

export function OrderHistory() {
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useInfiniteQuery({
    queryKey: ['orders'],
    queryFn: ({ pageParam }) => api.get<Page<Order>>('/orders', { after: pageParam }),
    getNextPageParam: (page) => page.next,
  })
  const orders = data?.pages.flatMap((p) => p.items)
  if (!orders) return <Spinner />
  return (
    <section>
      {orders.map((o) => <OrderRow key={o.id} order={o} />)}
      {hasNextPage && <Button onClick={() => fetchNextPage()} busy={isFetchingNextPage}>Load more</Button>}
    </section>
  )
}
"#;

/// Changed by !1291, its new version: the per-line rounding tested.
const VAT_TEST: &str = r#"import { describe, expect, it } from 'vitest'
import { roundCents, vatByRate } from './vat'
import type { InvoiceLine } from './vat'

/** A line of `qty` items at `price` excluding tax, at `rate`. */
const line = (price: number, qty: number, rate: number): InvoiceLine => ({
  sku: 'SKU-1',
  label: 'Item',
  unitPrice: price,
  quantity: qty,
  vatRate: rate,
})

describe('vat', () => {
  it('is empty without lines', () => {
    expect(vatByRate([]).size).toBe(0)
  })

  it('groups the lines by rate', () => {
    const rates = vatByRate([line(10, 1, 0.2), line(10, 1, 0.055), line(5, 2, 0.2)])
    expect([...rates.keys()].sort()).toEqual([0.055, 0.2])
  })

  it('multiplies by the quantity', () => {
    expect(vatByRate([line(10, 3, 0.2)]).get(0.2)).toBe(6)
  })

  it('computes a rate on one line', () => {
    // The simplest case: one line, one rate.
    expect(vatByRate([line(100, 1, 0.2)]).get(0.2)).toBe(20)
  })

  it('rounds each line before summing', () => {
    const lines = [line(16.66, 1, 0.2), line(16.66, 1, 0.2), line(16.66, 1, 0.2)]
    expect(vatByRate(lines).get(0.2)).toBe(9.99)
  })

  it('rounds half away from zero', () => {
    expect(roundCents(1.005)).toBe(1.01)
    expect(roundCents(-1.005)).toBe(-1.01)
  })
})
"#;

/// Changed by !1293, its new version: axios 1.7.9.
const CATALOG_PACKAGE: &str = r#"{
  "name": "@shop/catalog-api",
  "version": "2.13.2",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "tsc -p .",
    "dev": "tsx watch src/main.ts",
    "start": "node dist/main.js",
    "test": "vitest run",
    "lint": "eslint src"
  },
  "dependencies": {
    "@fastify/cors": "^9.0.1",
    "axios": "1.7.9",
    "fastify": "^4.28.1",
    "pg": "^8.12.0",
    "zod": "^3.23.8"
  },
  "devDependencies": {
    "tsx": "^4.19.1",
    "typescript": "^5.6.2",
    "vitest": "^2.1.2"
  }
}
"#;

/// Changed by !1294, its new version: the photo through ProductImage.
const PRODUCT_CARD: &str = r#"import { ProductImage } from './ProductImage'
import type { Product } from './types'

export function ProductCard({ product, eager }: { product: Product; eager?: boolean }) {
  return (
    <a href={`/p/${product.slug}`} className="card">
      <ProductImage src={product.photo} alt={product.name} eager={eager} />
      <h3>{product.name}</h3>
      <p className="price">{product.price.formatted}</p>
    </a>
  )
}
"#;

/// Changed by !1285, its new version: a failed webhook retried, then dead-lettered.
const RETRY: &str = r#"package webhooks

import (
	"context"
	"time"
)

// After 1, 4, 16 and 64 minutes, the dead letter queue.
const maxAttempts = 4

func backoff(attempt int) time.Duration {
	return time.Minute << (2 * attempt)
}

// Deliver sends the event to the merchant's endpoint; a failure is retried later, never lost.
// The queue keeps the attempt count with the event.
func (q *Queue) Deliver(ctx context.Context, e Event) error {
	if err := q.send(ctx, e); err != nil {
		if e.Attempt >= maxAttempts {
			return q.deadLetter(ctx, e, err)
		}
		return q.retryIn(ctx, e, backoff(e.Attempt))
	}
	return q.markDelivered(ctx, e)
}
"#;

/// Changed by !1280, its new version: a SKU found exactly, or by prefix.
const SEARCH: &str = r#"import { es } from './elastic'
import type { Product } from './types'

/** The products of the catalog, as the storefront and the back office search them. */
const INDEX = 'products'
/** At most this many results a page. */
const PAGE = 24

export interface SearchOptions {
  /** Only the products of this category (its slug). */
  category?: string
  /** Only those in stock. */
  inStock?: boolean
  page?: number
}

export interface SearchResult {
  items: Product[]
  total: number
}

/** The filters of the options, as Elasticsearch takes them. */
function filters(o: SearchOptions) {
  const out: object[] = []
  if (o.category) out.push({ term: { 'category.slug': o.category } })
  if (o.inStock) out.push({ range: { stock: { gt: 0 } } })
  return out
}

export async function search(q: string, o: SearchOptions = {}): Promise<SearchResult> {
  const page = Math.max(0, o.page ?? 0)
  const res = await es.search<Product>({
    index: INDEX, from: page * PAGE, size: PAGE,
    query: { bool: { must: searchQuery(q), filter: filters(o) } },
  })
  return { items: res.hits.hits.map((h) => h._source!), total: Number(res.hits.total) }
}

export function searchQuery(q: string) {
  if (/^[A-Z0-9-]{4,}$/i.test(q)) return { bool: { should: [{ term: { sku: q.toUpperCase() } }, { prefix: { sku: q.toUpperCase() } }] } }
  return { multi_match: { query: q, fields: ['name^3', 'description'] } }
}
"#;

/// Changed by !1288, its new version: the cart's two routes added.
const ROUTES: &str = r#"import type { App } from './app'
import { auth } from './auth/middleware'
import * as carts from './cart/cart.controller'
import * as customers from './customers/customers.controller'
import * as invoices from './invoices/invoices.controller'
import * as orders from './orders/orders.controller'

/**
 * Every route of orders-api. Each one behind `auth` but the health check; the cart is the
 * signed-in customer's own (SHOP-412), a guest's stays in the browser.
 */
export function health(app: App) {
  app.get('/health', (_req, res) => res.send({ ok: true }))
}

// Customers first: an order reads its customer.
// Then the orders, their invoices, and the saved cart.

export function routes(app: App) {
  app.get('/orders', auth, orders.list)
  app.get('/orders/:id', auth, orders.get)
  app.post('/orders', auth, orders.create)
  app.get('/cart', auth, carts.get)
  app.put('/cart', auth, carts.save)

  app.get('/invoices/:id.pdf', auth, invoices.pdf)
  app.get('/invoices/:id', auth, invoices.get)

  app.get('/customers/me', auth, customers.me)
  app.patch('/customers/me', auth, customers.update)
}
"#;

const VAT: &str = r#"export interface InvoiceLine {
  sku: string
  quantity: number
  unitPrice: number
  /** 0.2 for 20 %. */
  rate: number
}

/** VAT of an invoice, by rate. */
export function vatByRate(lines: InvoiceLine[]): Map<number, number> {
  const out = new Map<number, number>()
  for (const l of lines) {
    // Rounded per line, as on the printed invoice: the totals add up to the cent.
    const vat = roundCents(l.unitPrice * l.quantity * l.rate)
    out.set(l.rate, (out.get(l.rate) ?? 0) + vat)
  }
  return out
}

/** Half away from zero, on cents: `Math.round` alone turns 1.005 into 1.00. */
export const roundCents = (n: number) => (Math.sign(n) * Math.round((Math.abs(n) + Number.EPSILON) * 100)) / 100
"#;

const PRODUCT_IMAGE: &str = r#"import Image from 'next/image'

/** A product's photo, loaded when it nears the viewport; the first row of a page loads at once. */
export function ProductImage({ src, alt, eager = false }: { src: string; alt: string; eager?: boolean }) {
  return (
    <Image
      src={src}
      alt={alt}
      width={480}
      height={480}
      sizes="(max-width: 768px) 50vw, 240px"
      loading={eager ? 'eager' : 'lazy'}
      placeholder="blur"
      blurDataURL={`${src}?w=16&q=30`}
    />
  )
}
"#;

const CSV_EXPORT: &str = r#"import type { Order } from '../api/types'

const COLUMNS = ['Order', 'Date', 'Customer', 'Items', 'Total excl. VAT', 'VAT', 'Total', 'Status'] as const

/** Orders as CSV for Excel: `;`-separated, decimal comma, and a BOM so that accents survive. */
export function ordersCsv(orders: Order[]): string {
  const money = (n: number) => n.toFixed(2).replace('.', ',')
  const cell = (v: string) => (/[;"\n]/.test(v) ? `"${v.replaceAll('"', '""')}"` : v)
  const rows = orders.map((o) =>
    [o.number, o.createdAt.slice(0, 10), o.customer.name, String(o.items), money(o.net), money(o.vat), money(o.total), o.status].map(cell).join(';'),
  )
  return '﻿' + [COLUMNS.join(';'), ...rows].join('\r\n')
}
"#;

const MULTI_CURRENCY: &str = r#"# SHOP-441 · Multi-currency prices

## Need

Customers in Switzerland and the UK pay in their currency, at a price the shop sets, not a
converted one that changes every day.

## Proposal

- A **price list per currency** (`EUR`, `CHF`, `GBP`): `catalog-api` returns the customer's.
- The currency follows the **delivery country**, chosen in the header; it is kept in the cart.
- Orders store the currency and the amounts as paid; invoices print them as is.
- No price for a currency: the product is not sold in it (hidden from the catalog).

## Impact

| Component | Change | Estimate |
|---|---|---|
| catalog-api | `prices` table keyed by `(sku, currency)`, endpoint filter | 2 d |
| orders-api | `currency` on carts, orders and invoices | 1.5 d |
| payments-api | currency passed to the provider | 0.5 d |
| storefront-webapp | country picker, formatting | 2 d |
| back office | price list import (CSV) | 1 d |

## Open questions

1. Do promotions get a value per currency, or a percentage only?
2. Accounting: one ledger per currency, or converted at month end?
"#;

const CHANGELOG: &str = r#"# Changelog

## Unreleased

### Storefront

- The cart of a signed-in customer is kept between sessions and devices (SHOP-412).

### Orders API

- Payment webhooks are retried with an exponential backoff (SHOP-421).
- `GET /cart` and `PUT /cart`: the signed-in customer's cart (SHOP-412).

## 2026.09

### Catalog API

- Search by SKU, exact or prefix (SHOP-430).

### Storefront

- New checkout layout: address, delivery and payment on one page (SHOP-399).
"#;

const README: &str = r#"# Shop

The online shop of Example Org: a Next.js storefront, three APIs and a back office.

| Service | Path | Stack |
|---|---|---|
| storefront-webapp | `services/storefront-webapp` | Next.js 14 |
| catalog-api | `services/catalog-api` | Node 22, Fastify |
| orders-api | `services/orders-api` | Node 22, Fastify, Postgres |
| payments-api | `services/payments-api` | Go 1.22 |
| backoffice | `services/backoffice` | React, Vite |

## Run it

```sh
docker compose up -d postgres
npm ci
npm run dev
```

The storefront answers on http://localhost:3000.

## Release

`release/<year>.<month>` deploys to staging on every merge. A tag `<component>/rc-<version>`
deploys to preprod, `<component>/<version>` to production.
"#;

/// An MR's description, threads and files, by repository and number.
struct MrContent {
    description: &'static str,
    threads: Vec<Discussion>,
    files: Vec<FileDiff>,
}

/// A thread: (author index, text, minutes ago) for each note.
fn thread(id: &str, resolved: Option<bool>, at: Option<(&str, u64)>, notes: &[(usize, &str, i64)]) -> Discussion {
    Discussion {
        id: id.into(),
        individual: resolved.is_none() && at.is_none(),
        resolvable: resolved.is_some(),
        resolved: resolved.unwrap_or(false),
        path: at.map(|(p, _)| p.into()),
        line: at.map(|(_, l)| l),
        old_side: false,
        start_line: None,
        notes: notes
            .iter()
            .enumerate()
            .map(|(i, &(who, body, minutes))| {
                let (author, author_name) = login(who);
                Note { id: id.bytes().map(u64::from).sum::<u64>() * 10 + i as u64, author, author_name, body: body.into(), created_at: ago(minutes) }
            })
            .collect(),
    }
}

fn mr_content(repo: Option<&str>, iid: u64) -> Result<MrContent> {
    Ok(match (repo, iid) {
        (None, 1288) => MrContent {
            description: "Closes [SHOP-412](https://app.clickup.example.org/t/SHOP-412).\n\n## What\n\nA signed-in customer's cart is kept by orders-api, so it survives a new session or another device. A guest's cart stays in the browser and is **merged** into the account's at sign-in.\n\n## How to test\n\n1. Sign in on the review app, add two products.\n2. Sign in from a private window: the cart is there.\n3. Sign out, add a product as a guest, sign in again: three products.\n\n## Checklist\n\n- [x] Migration is reversible\n- [x] CHANGELOG\n- [ ] Merge of the guest cart covered by a test",
            threads: vec![
                thread("demo-1288-1", Some(false), Some(("services/orders-api/src/cart/cart.repository.ts", 17)), &[
                    (1, "Deleting and reinserting every line resets `added_at`: the cart's order changes on each save. An upsert on `(customer_id, sku)` would keep it, and the primary key is already there.", 140),
                    (0, "Right, and it avoids a delete in each transaction. I'll switch to `INSERT … ON CONFLICT`.", 95),
                ]),
                thread("demo-1288-2", Some(false), None, &[(1, "Could you add a test for the merge at sign-in? Same SKU in both carts: quantities add up, or the account's wins?", 120)]),
                thread("demo-1288-3", None, None, &[(3, "Tried it on the review app: works on Safari and Firefox 👍", 60)]),
            ],
            files: vec![
                changed("services/storefront-webapp/src/cart/useCart.ts", &[
                    hunk(1, 1, "", "
                        | import { useEffect, useState } from 'react'
                        +import { api } from '../api'
                        +import { useSession } from '../account/useSession'
                        | import type { Cart } from './types'
                        -import { emptyCart } from './cart'
                        +import { emptyCart, mergeCarts, readLocalCart } from './cart'
                        | "),
                    hunk(12, 14, "export function useCart() {", "
                        |   const [cart, setCart] = useState<Cart>(emptyCart)
                        +  const { customer } = useSession()
                        |
                        +  // Signed in: the cart is the account's, kept by orders-api between sessions.
                        |   useEffect(() => {
                        -    const saved = localStorage.getItem('cart')
                        -    if (saved) setCart(JSON.parse(saved))
                        -  }, [])
                        +    if (customer) api.get<Cart>('/cart').then((saved) => setCart(mergeCarts(saved, readLocalCart())))
                        +    else setCart(readLocalCart())
                        +  }, [customer])
                        |
                        |   const update = (next: Cart) => {
                        |     setCart(next)
                        -    localStorage.setItem('cart', JSON.stringify(next))
                        +    if (customer) void api.put('/cart', next)
                        +    else localStorage.setItem('cart', JSON.stringify(next))
                        |   }
                        "),
                ]),
                changed("services/orders-api/src/routes.ts", &[hunk(20, 20, "export function routes(app: App) {", "
                    |   app.get('/orders', auth, orders.list)
                    |   app.get('/orders/:id', auth, orders.get)
                    |   app.post('/orders', auth, orders.create)
                    +  app.get('/cart', auth, carts.get)
                    +  app.put('/cart', auth, carts.save)
                    |
                    |   app.get('/invoices/:id.pdf', auth, invoices.pdf)
                    ")]),
                added("services/orders-api/src/cart/cart.repository.ts"),
                added("services/orders-api/migrations/2026_10_06_cart_lines.sql"),
                changed("CHANGELOG.md", &[hunk(1, 1, "", "
                    | # Changelog
                    |
                    | ## Unreleased
                    |
                    +### Storefront
                    +
                    +- The cart of a signed-in customer is kept between sessions and devices (SHOP-412).
                    +
                    | ### Orders API
                    |
                    | - Payment webhooks are retried with an exponential backoff (SHOP-421).
                    +- `GET /cart` and `PUT /cart`: the signed-in customer's cart (SHOP-412).
                    ")]),
            ],
        },
        (None, 1291) => MrContent {
            description: "Closes [SHOP-433](https://app.clickup.example.org/t/SHOP-433).\n\nInvoices summed the VAT of all lines, then rounded: the printed lines did not add up to the total (off by a cent on 1 invoice in 40). The VAT is now rounded **per line**, half away from zero.\n\n> ⚠️ One test still fails: the sum of rounded floats is not exactly `9.99`.",
            threads: vec![thread("demo-1291-1", Some(false), Some(("services/orders-api/src/invoices/vat.ts", 14)), &[
                (2, "Summing floats will bite again (it already does in the test). Could we sum in integer cents and divide once at the end?", 12),
            ])],
            files: vec![
                changed("services/orders-api/src/invoices/vat.ts", &[hunk(9, 9, "export interface InvoiceLine {", "
                    | /** VAT of an invoice, by rate. */
                    | export function vatByRate(lines: InvoiceLine[]): Map<number, number> {
                    |   const out = new Map<number, number>()
                    |   for (const l of lines) {
                    -    out.set(l.rate, (out.get(l.rate) ?? 0) + l.unitPrice * l.quantity * l.rate)
                    +    // Rounded per line, as on the printed invoice: the totals add up to the cent.
                    +    const vat = roundCents(l.unitPrice * l.quantity * l.rate)
                    +    out.set(l.rate, (out.get(l.rate) ?? 0) + vat)
                    |   }
                    -  for (const [rate, vat] of out) out.set(rate, Math.round(vat * 100) / 100)
                    |   return out
                    | }
                    +
                    +/** Half away from zero, on cents: `Math.round` alone turns 1.005 into 1.00. */
                    +export const roundCents = (n: number) => (Math.sign(n) * Math.round((Math.abs(n) + Number.EPSILON) * 100)) / 100
                    ")]),
                changed("services/orders-api/src/invoices/vat.test.ts", &[
                    hunk(1, 1, "", "
                        | import { describe, expect, it } from 'vitest'
                        -import { vatByRate } from './vat'
                        +import { roundCents, vatByRate } from './vat'
                        "),
                    hunk(30, 30, "describe('vat', () => {", "
                        |     expect(vatByRate([line(100, 1, 0.2)]).get(0.2)).toBe(20)
                        |   })
                        |
                        +  it('rounds each line before summing', () => {
                        +    const lines = [line(16.66, 1, 0.2), line(16.66, 1, 0.2), line(16.66, 1, 0.2)]
                        +    expect(vatByRate(lines).get(0.2)).toBe(9.99)
                        +  })
                        +
                        +  it('rounds half away from zero', () => {
                        +    expect(roundCents(1.005)).toBe(1.01)
                        +    expect(roundCents(-1.005)).toBe(-1.01)
                        +  })
                        | })
                        "),
                ]),
            ],
        },
        (None, 1290) => MrContent {
            description: "Closes [SHOP-418](https://app.clickup.example.org/t/SHOP-418).\n\nThe order history loaded every order at once (up to 900 for our best customers). It now loads **20 at a time**, with a cursor on `(created_at, id)`, and the page shows a *Load more* button.\n\n| Orders | Before | After |\n|---|---|---|\n| 50 | 180 ms | 40 ms |\n| 900 | 2.4 s | 45 ms |",
            threads: vec![thread("demo-1290-1", Some(true), Some(("services/orders-api/src/orders/orders.repository.ts", 30)), &[
                (2, "A cursor on `created_at` alone can skip orders created in the same millisecond (imports do that).", 400),
                (1, "Good catch: the cursor is now `(created_at, id)`, with a test of two orders at the same instant.", 330),
            ])],
            files: vec![
                changed("services/orders-api/src/orders/orders.repository.ts", &[hunk(24, 24, "export class OrdersRepository {", "
                    -  async listByCustomer(customerId: string): Promise<Order[]> {
                    -    return sql<Order[]>`SELECT * FROM orders WHERE customer_id = ${customerId} ORDER BY created_at DESC`
                    +  /** A page of a customer's orders, newest first; `after`: the last order of the previous page. */
                    +  async listByCustomer(customerId: string, after?: Cursor, limit = 20): Promise<Page<Order>> {
                    +    const rows = await sql<Order[]>`
                    +      SELECT * FROM orders
                    +      WHERE customer_id = ${customerId}
                    +        AND (${after ?? null}::text IS NULL OR (created_at, id) < (${after?.createdAt}, ${after?.id}))
                    +      ORDER BY created_at DESC, id DESC
                    +      LIMIT ${limit + 1}`
                    +    return { items: rows.slice(0, limit), next: rows.length > limit ? cursorOf(rows[limit - 1]) : null }
                    |   }
                    ")]),
                changed("services/storefront-webapp/src/account/OrderHistory.tsx", &[hunk(8, 8, "export function OrderHistory() {", "
                    -  const { data: orders } = useQuery(['orders'], () => api.get<Order[]>('/orders'))
                    +  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useInfiniteQuery({
                    +    queryKey: ['orders'],
                    +    queryFn: ({ pageParam }) => api.get<Page<Order>>('/orders', { after: pageParam }),
                    +    getNextPageParam: (page) => page.next,
                    +  })
                    +  const orders = data?.pages.flatMap((p) => p.items)
                    |   if (!orders) return <Spinner />
                    |   return (
                    |     <section>
                    |       {orders.map((o) => <OrderRow key={o.id} order={o} />)}
                    +      {hasNextPage && <Button onClick={() => fetchNextPage()} busy={isFetchingNextPage}>Load more</Button>}
                    |     </section>
                    |   )
                    ")]),
            ],
        },
        (None, 1293) => MrContent {
            description: "This MR contains the following updates:\n\n| Package | Change |\n|---|---|\n| [axios](https://axios-http.com) | `1.7.4` → `1.7.9` |\n\n---\n\n### Release notes\n\n**v1.7.9**: fixes a regression with `fetch` adapters and custom headers.\n\n**v1.7.8**: allows `fetchOptions` in the request config.\n\n---\n\n📅 **Schedule**: at any time.\n\n🚦 **Automerge**: disabled by config.\n\n♻ **Rebasing**: whenever the MR becomes conflicted.\n\n*This MR was generated by Renovate Bot.*",
            threads: vec![thread("demo-1293-1", None, None, &[(4, "Changelog checked, nothing breaking for us. Approved.", 80)])],
            files: vec![
                changed("services/catalog-api/package.json", &[hunk(14, 14, "\"dependencies\": {", "
                    |     \"@fastify/cors\": \"^9.0.1\",
                    -    \"axios\": \"1.7.4\",
                    +    \"axios\": \"1.7.9\",
                    |     \"fastify\": \"^4.28.1\",
                    ")]),
                FileDiff { collapsed: true, diff: String::new(), ..changed("package-lock.json", &[]) },
            ],
        },
        (None, 1294) => MrContent {
            description: "Part of [SHOP-425](https://app.clickup.example.org/t/SHOP-425). **Draft**: the category pages are done, the search results are not.\n\nProduct photos load when they near the viewport, with a blurred placeholder. The first row stays eager, so the LCP does not regress.",
            threads: Vec::new(),
            files: vec![
                added("services/storefront-webapp/src/catalog/ProductImage.tsx"),
                changed("services/storefront-webapp/src/catalog/ProductCard.tsx", &[hunk(1, 1, "", "
                    -import Image from 'next/image'
                    +import { ProductImage } from './ProductImage'
                    | import type { Product } from './types'
                    |
                    -export function ProductCard({ product }: { product: Product }) {
                    +export function ProductCard({ product, eager }: { product: Product; eager?: boolean }) {
                    |   return (
                    |     <a href={`/p/${product.slug}`} className=\"card\">
                    -      <Image src={product.photo} alt={product.name} width={480} height={480} />
                    +      <ProductImage src={product.photo} alt={product.name} eager={eager} />
                    |       <h3>{product.name}</h3>
                    ")]),
                // Binary: its diff is empty, the window shows it before and after (`mr_image`).
                FileDiff { collapsed: true, diff: String::new(), ..changed(PLACEHOLDER, &[]) },
            ],
        },
        (None, 1292) => MrContent {
            description: "Closes [SHOP-427](https://app.clickup.example.org/t/SHOP-427).\n\nThe back office's order list gets an **Export CSV** button: the orders of the current filter, opened in Excel as is (semicolons, decimal comma, BOM).",
            threads: vec![thread("demo-1292-1", Some(true), Some(("services/backoffice/src/orders/export.ts", 12)), &[
                (0, "Excel on Windows shows `Ã©` for accents without a BOM.", 900),
                (2, "Added `\\uFEFF` at the start, checked with a customer named Hélène 🙂", 760),
            ])],
            files: vec![added("services/backoffice/src/orders/export.ts")],
        },
        (None, 1284) => MrContent {
            description: "Closes [SHOP-409](https://app.clickup.example.org/t/SHOP-409).\n\nSeveral coupons can apply to one cart: the percentage ones first, then the fixed amounts, never below zero.",
            threads: vec![thread("demo-1284-1", None, None, &[(3, "Rules read well. Approved.", 950)])],
            files: vec![added("services/checkout/src/coupons/stack.ts")],
        },
        (None, 1286) => MrContent {
            description: "Closes [SHOP-415](https://app.clickup.example.org/t/SHOP-415).\n\nThe cart shows the next delivery slots of the customer's area, refreshed every minute.",
            threads: vec![thread("demo-1286-1", None, None, &[(1, "Nice. Approved.", 640)])],
            files: vec![added("apps/storefront/src/cart/DeliverySlots.tsx")],
        },
        (None, 1289) => MrContent {
            description: "Closes [SHOP-422](https://app.clickup.example.org/t/SHOP-422).\n\nOrderService is split in three: pricing, fulfilment and history. No behaviour change.",
            threads: vec![thread("demo-1289-1", None, None, &[(4, "Big but mechanical. Approved.", 230)])],
            files: vec![added("services/orders/src/pricing.ts")],
        },
        (Some("helm"), 88) => MrContent {
            description: "orders-api `1.8.0` is OOMKilled in preprod since the saved carts: the cart cache holds more. Raises the request to 640 Mi and the limit to 768 Mi, in preprod and production.\n\nGrafana: the heap peaks at 590 Mi during the 08:00 import.",
            threads: vec![thread("demo-88-1", None, None, &[(2, "LGTM. Let's watch the restarts after the rollout.", 110)])],
            files: ["preprod", "prod"]
                .iter()
                .map(|env| {
                    changed(&format!("charts/orders-api/values-{env}.yaml"), &[hunk(14, 14, "orders-api:", "
                        |   resources:
                        |     requests:
                        |       cpu: 250m
                        -      memory: 512Mi
                        +      memory: 640Mi
                        |     limits:
                        |       cpu: \"1\"
                        -      memory: 512Mi
                        +      memory: 768Mi
                        ")])
                })
                .collect(),
        },
        (Some("toolkit"), 14) => MrContent {
            description: "`sync.sh --dry-run` lists what it would copy and delete, without touching anything.",
            threads: Vec::new(),
            files: vec![changed("sync.sh", &[hunk(3, 3, "set -euo pipefail", "
                +DRY_RUN=0
                +[[ \"${1:-}\" == --dry-run ]] && DRY_RUN=1
                | SRC=\"$HOME/dotfiles\"
                ")])],
        },
        (None, 1285) => MrContent {
            description: "Closes [SHOP-421](https://app.clickup.example.org/t/SHOP-421).\n\nA payment webhook that fails is retried after 1, 4, 16 and 64 minutes, then lands in the dead letter queue the back office shows.",
            threads: Vec::new(),
            files: vec![changed("services/payments-api/webhooks/retry.go", &[hunk(18, 18, "func (q *Queue) Deliver(ctx context.Context, e Event) error {", "
                | 	if err := q.send(ctx, e); err != nil {
                -		return err
                +		if e.Attempt >= maxAttempts {
                +			return q.deadLetter(ctx, e, err)
                +		}
                +		return q.retryIn(ctx, e, backoff(e.Attempt))
                | 	}
                ")])],
        },
        (None, 1280) => MrContent {
            description: "Closes [SHOP-430](https://app.clickup.example.org/t/SHOP-430).\n\nThe catalog search matches a SKU exactly, or by prefix from 4 characters.",
            threads: Vec::new(),
            files: vec![changed("services/catalog-api/src/search.ts", &[hunk(40, 40, "export function searchQuery(q: string) {", "
                +  if (/^[A-Z0-9-]{4,}$/i.test(q)) return { bool: { should: [{ term: { sku: q.toUpperCase() } }, { prefix: { sku: q.toUpperCase() } }] } }
                |   return { multi_match: { query: q, fields: ['name^3', 'description'] } }
                ")])],
        },
        _ => bail!("!{iid} is not a merge request of the demo"),
    })
}

fn find_mr<'a>(s: &'a Snapshot, repo: Option<&str>, iid: u64) -> Result<&'a crate::state::MergeRequest> {
    s.merge_requests.iter().find(|m| m.iid == iid && m.repo.as_deref() == repo).with_context(|| format!("!{iid} is not a merge request Prism displays"))
}

pub fn mr_detail(s: &Snapshot, repo: Option<&str>, iid: u64) -> Result<MrDetail> {
    let mr = find_mr(s, repo, iid)?;
    let c = mr_content(repo, iid)?;
    let me = if mr.forge.is_some() { &s.me_elsewhere } else { &s.me }.clone().unwrap_or_default();
    Ok(MrDetail {
        description: c.description.into(),
        discussions: c.threads,
        user_has_approved: mr.approved_by.contains(&me),
        changes_requested_by: mr.changes_requested_by.clone(),
        user_requested_changes: mr.changes_requested_by.contains(&me),
        user_can_approve: mr.author != me && mr.state == crate::state::MrState::Opened,
        approved_by: mr.approved_by.clone(),
        approvals_left: mr.approvals_left as i64,
        approval_history: demo_history(mr),
    })
}

/// A plausible history ending in the MR's state: a first push, an approval reset by a second push,
/// then today's approvals and requests for changes.
fn demo_history(mr: &crate::state::MergeRequest) -> Vec<ApprovalEvent> {
    let at = mr.updated_at;
    let event = |hours: i64, kind, who: &str, whom: Option<&str>, commits| ApprovalEvent {
        at: at - Duration::hours(hours),
        kind,
        author: who.into(),
        author_name: who.into(),
        whom: whom.map(Into::into),
        commits,
    };
    let mut out = vec![event(30, ApprovalKind::Pushed, &mr.author, None, 3)];
    let late: Vec<&String> = mr.approved_by.iter().chain(&mr.changes_requested_by).collect();
    if let Some(first) = late.first() {
        out.push(event(26, ApprovalKind::Approved, first, None, 0));
        out.push(event(8, ApprovalKind::Pushed, &mr.author, None, 1));
        out.push(event(8, ApprovalKind::Reset, &mr.author, Some(&format!("@{first}")), 0));
    }
    for (i, who) in mr.approved_by.iter().enumerate() {
        out.push(event(3 - i as i64, ApprovalKind::Approved, who, None, 0));
    }
    for who in &mr.changes_requested_by {
        out.push(event(1, ApprovalKind::ChangesRequested, who, None, 0));
    }
    approval_history(out)
}

pub fn mr_diff(s: &Snapshot, repo: Option<&str>, iid: u64) -> Result<MrDiff> {
    find_mr(s, repo, iid)?;
    Ok(MrDiff { files: mr_content(repo, iid)?.files, truncated: false })
}

/// A few commits made up from its title, the last one an hour ago.
pub fn mr_commits(s: &Snapshot, repo: Option<&str>, iid: u64) -> Result<Vec<MrCommit>> {
    let mr = find_mr(s, repo, iid)?;
    let now = Utc::now();
    let titles = [mr.title.clone(), "Address review comments".into(), "Add tests".into(), "First pass".into()];
    Ok(titles
        .into_iter()
        .enumerate()
        .map(|(i, title)| MrCommit {
            sha: format!("{:08x}{:032x}", 0x5ee0_0000u32 + (iid as u32) * 16 + i as u32, i),
            title,
            author: mr.author.clone(),
            at: Some(now - Duration::hours(1 + 5 * i as i64)),
            url: None,
        })
        .collect())
}

pub fn mr_file(s: &Snapshot, repo: Option<&str>, iid: u64, path: &str) -> Result<String> {
    find_mr(s, repo, iid)?;
    file(path).map(String::from).with_context(|| format!("{path}: not in the demo"))
}

/// The image of the demo's diffs: a product photo's placeholder, a flat grey before, blurred colours after.
const PLACEHOLDER: &str = "services/storefront-webapp/public/placeholder.png";

/// An image of an MR's diff, as a `data:` URL: the demo has one, drawn here.
pub fn mr_image(s: &Snapshot, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String> {
    find_mr(s, repo, iid)?;
    if path != PLACEHOLDER {
        bail!("{path}: not an image of the demo");
    }
    let (w, h) = (240u32, 160u32);
    let pixel = |x: u32, y: u32| -> [u8; 3] {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        if old {
            let v = if (x + y) % 24 < 12 { 0xd4 } else { 0xcc };
            [v, v, v]
        } else {
            // Soft blobs of a product shot, as a blurred preview looks.
            let blob = |cx: f32, cy: f32, r: f32| (1.0 - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() / r).max(0.0);
            let (a, b) = (blob(0.35, 0.45, 0.45), blob(0.75, 0.6, 0.35));
            let mix = |base: f32, ca: f32, cb: f32| (base + a * ca + b * cb).clamp(0.0, 255.0) as u8;
            [mix(236.0 - 40.0 * fy, -20.0, -120.0), mix(226.0 - 30.0 * fy, -90.0, -40.0), mix(214.0, -150.0, 20.0)]
        }
    };
    crate::forge::image_data_url(&png(w, h, pixel))
}

/// A PNG of `w` × `h` RGB pixels, its data stored uncompressed: enough for a sample image.
fn png(w: u32, h: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = !0u32;
        for b in bytes {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 == 1 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let c = crc(&out[start..]);
        out.extend(c.to_be_bytes());
    }
    let mut raw = Vec::new();
    for y in 0..h {
        raw.push(0); // no filter
        for x in 0..w {
            raw.extend(pixel(x, y));
        }
    }
    // zlib, in stored blocks of at most 65535 bytes, then its Adler-32.
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(0xffff).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for x in &raw {
        a = (a + *x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut head = Vec::new();
    head.extend(w.to_be_bytes());
    head.extend(h.to_be_bytes());
    head.extend([8, 2, 0, 0, 0]); // 8 bits, RGB
    chunk(&mut out, b"IHDR", &head);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn members() -> Vec<Member> {
    (0..5)
        .map(|i| {
            let (username, name) = login(i);
            Member { id: i as u64 + 1, username, name, avatar_url: None }
        })
        .collect()
}

/// The demo project's labels: those its MRs carry, and a few more to pick from.
pub fn labels(s: &Snapshot) -> Vec<ForgeLabel> {
    let colors = ["#6699cc", "#cc5a5a", "#5cb85c", "#d9a43a", "#9c6ade", "#3aa6a6"];
    let more = ["backend", "frontend", "bug", "documentation", "needs-design", "tech-debt"];
    let mut names: Vec<String> = s.merge_requests.iter().flat_map(|m| m.labels.iter().cloned()).chain(more.iter().map(|l| l.to_string())).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names.into_iter().enumerate().map(|(i, name)| ForgeLabel { name, color: Some(colors[i % colors.len()].into()), description: None }).collect()
}

/* ---------- Pipelines and logs ---------- */

/// Every job of the demo's pipelines: branches, MRs, the rollout, the Helm deploy, Renovate.
fn jobs(s: &Snapshot) -> Vec<JobInfo> {
    let mut out = Vec::new();
    let pipelines = s.pipelines.iter().map(|p| (p, None)).chain(s.merge_requests.iter().filter_map(|m| m.head_pipeline.as_ref().map(|p| (p, Some(m.source_branch.as_str())))));
    for (p, branch) in pipelines {
        let r#ref = branch.unwrap_or(&p.target).to_string();
        for (si, stage) in p.stages.iter().enumerate() {
            let names: Vec<(u64, String)> = if stage.manual_jobs.is_empty() {
                STAGE_JOBS.get(si).copied().unwrap_or(&[]).iter().take(stage.jobs as usize).enumerate().map(|(j, n)| (job_id(p.id, si, j), n.to_string())).collect()
            } else {
                stage.manual_jobs.iter().map(|m| (m.id, m.name.clone())).collect()
            };
            for (j, (id, name)) in names.into_iter().enumerate() {
                let status = match stage.status.name() {
                    "failed" if stage.failed_jobs.iter().any(|f| f.name == name) => "failed",
                    "failed" | "success" => "success",
                    "running" if (j as u32) < stage.finished => "success",
                    "running" if j as u32 == stage.finished => "running",
                    "running" => "pending",
                    other => other,
                };
                let started_at = (status != "pending" && status != "manual").then(|| stage.started_at.unwrap_or(p.updated_at) + Duration::seconds(j as i64 * 7));
                let finished_at = matches!(status, "success" | "failed").then(|| started_at.unwrap_or(p.updated_at) + Duration::seconds(95 + (id % 13) as i64 * 23));
                out.push(job(id, &name, &stage.name, status, started_at, finished_at, p.id, &r#ref, &p.url));
            }
        }
    }
    if let Some(run) = s.renovate.as_ref().and_then(|r| r.run.as_ref()) {
        out.push(job(run.id + 1, "renovate", "renovate", run.status.name(), Some(run.started_at), Some(run.updated_at), run.id, "main", &run.url));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn job(id: u64, name: &str, stage: &str, status: &str, started_at: Option<DateTime<Utc>>, finished_at: Option<DateTime<Utc>>, pipeline: u64, r#ref: &str, pipeline_url: &str) -> JobInfo {
    let project = if pipeline_url.contains(HELM) { HELM } else { PROJECT };
    JobInfo {
        id,
        name: name.into(),
        stage: stage.into(),
        status: crate::state::PipelineStatus::of(status),
        allow_failure: name == "sast",
        duration: started_at.map(|s| (finished_at.unwrap_or_else(now) - s).num_seconds() as f64),
        started_at,
        finished_at,
        url: format!("{HOST}/{project}/-/jobs/{id}"),
        pipeline_id: pipeline,
        r#ref: r#ref.into(),
    }
}

pub fn pipeline_jobs(s: &Snapshot, pipeline: u64) -> Result<Vec<JobInfo>> {
    let out: Vec<JobInfo> = jobs(s).into_iter().filter(|j| j.pipeline_id == pipeline).collect();
    if out.is_empty() {
        bail!("pipeline {pipeline} is not in the demo");
    }
    Ok(out)
}

pub fn job_log(s: &Snapshot, id: u64) -> Result<JobLog> {
    let job = jobs(s).into_iter().find(|j| j.id == id).with_context(|| format!("job {id} is not in the demo"))?;
    let log = log_of(&job);
    Ok(JobLog { job, log, truncated: false })
}

const GREEN: &str = "\x1b[32;1m";
const RED: &str = "\x1b[31;1m";
const YELLOW: &str = "\x1b[33;1m";
const DIM: &str = "\x1b[90m";
const OFF: &str = "\x1b[0;m";

/// A job's log, as GitLab prints it: the runner's preamble, the job's commands, the outcome.
fn log_of(j: &JobInfo) -> String {
    if j.status.name() == "manual" || j.status.name() == "created" || j.status.name() == "pending" {
        return String::new();
    }
    let image = if j.name.starts_with("helm") || j.name.starts_with("argocd") { "alpine/helm:3.16.2" } else if j.name == "docker-image" { "gcr.io/kaniko-project/executor:v1.23.2-debug" } else if j.name == "renovate" { "renovate/renovate:39" } else { "node:22-alpine" };
    let mut log = format!(
        "{DIM}Running with gitlab-runner 17.5.2 (c5a8a8b2){OFF}\n{DIM}  on runner-x7k2 shop-k8s, system ID: s_4a1c9e2f{OFF}\n\x1b[0K{GREEN}Preparing the \"kubernetes\" executor{OFF}\nUsing Kubernetes namespace: gitlab-runners\nUsing Kubernetes executor with image {image} ...\n\x1b[0K{GREEN}Preparing environment{OFF}\nWaiting for pod gitlab-runners/runner-x7k2-project-12-concurrent-0 to be running, status is Pending\nRunning on runner-x7k2-project-12-concurrent-0 via gitlab-runner-6c9f7d-x4kq...\n\x1b[0K{GREEN}Getting source from Git repository{OFF}\nFetching changes with git depth set to 20...\nInitialized empty Git repository in /builds/example-org/shop/.git/\nChecking out 4f2a9c1e as detached HEAD (ref is {})...\n\x1b[0K{GREEN}Executing \"step_script\" stage of the job script{OFF}\n",
        j.r#ref
    );
    let cmd = |c: &str| format!("{GREEN}$ {c}{OFF}\n");
    let body = match j.name.as_str() {
        "unit-tests" if j.status.name() == "failed" => format!(
            "{}{}\n> shop@2026.10.0 test\n> vitest run --reporter=dot\n\n RUN  v2.1.4 /builds/example-org/shop/services/orders-api\n\n ✓ src/orders/orders.service.test.ts (24 tests) 412ms\n ✓ src/cart/cart.service.test.ts (11 tests) 133ms\n ✓ src/invoices/pdf.test.ts (6 tests) 820ms\n {RED}❯{OFF} src/invoices/vat.test.ts (9 tests | {RED}1 failed{OFF}) 61ms\n   {RED}×{OFF} vat > rounds each line before summing\n\n{RED}⎯⎯⎯⎯⎯⎯⎯ Failed Tests 1 ⎯⎯⎯⎯⎯⎯⎯{OFF}\n\n {RED}FAIL{OFF}  src/invoices/vat.test.ts > vat > rounds each line before summing\n{RED}AssertionError: expected 9.990000000000002 to be 9.99 // Object.is equality{OFF}\n\n{RED}- Expected{OFF}\n{GREEN}+ Received{OFF}\n\n{RED}- 9.99{OFF}\n{GREEN}+ 9.990000000000002{OFF}\n\n ❯ src/invoices/vat.test.ts:35:41\n     33|   it('rounds each line before summing', () => {{\n     34|     const lines = [line(16.66, 1, 0.2), line(16.66, 1, 0.2), line(16.66, 1, 0.2)]\n     35|     expect(vatByRate(lines).get(0.2)).toBe(9.99)\n       |                                         ^\n     36|   }})\n\n Test Files  {RED}1 failed{OFF} | {GREEN}23 passed{OFF} (24)\n      Tests  {RED}1 failed{OFF} | {GREEN}311 passed{OFF} (312)\n   Duration  6.41s\n\n",
            cmd("npm ci --prefer-offline"),
            cmd("npm run test -w services/orders-api")
        ),
        "unit-tests" if j.status.name() == "running" => format!(
            "{}{}\n RUN  v2.1.4 /builds/example-org/shop/services/storefront-webapp\n\n ✓ src/cart/useCart.test.tsx (8 tests) 210ms\n ✓ src/catalog/ProductCard.test.tsx (5 tests) 96ms\n",
            cmd("npm ci --prefer-offline"),
            cmd("npm run test -w services/storefront-webapp")
        ),
        "unit-tests" => format!("{}{}\n Test Files  {GREEN}24 passed{OFF} (24)\n      Tests  {GREEN}314 passed{OFF} (314)\n   Duration  6.12s\n\n", cmd("npm ci --prefer-offline"), cmd("npm run test --workspaces")),
        "lint" => format!("{}{}\n✔ No ESLint warnings or errors\n{}Checking formatting...\nAll matched files use Prettier code style!\n", cmd("npm ci --prefer-offline"), cmd("npm run lint --workspaces"), cmd("npx prettier --check .")),
        "typecheck" => format!("{}{}\n> tsc -b services/*/tsconfig.json\n\n", cmd("npm ci --prefer-offline"), cmd("npm run typecheck")),
        "integration-tests" => format!("{}{}\nWaiting for postgres:5432... ready\n ✓ orders: create, pay, invoice (1.8 s)\n ✓ cart: save and reload (0.4 s)\n ✓ webhooks: retried with backoff (2.1 s)\n\n  {GREEN}38 passing{OFF} (14s)\n", cmd("docker compose -f ci/compose.yml up -d postgres"), cmd("npm run test:integration")),
        "sast" => format!("{}\n{YELLOW}[WARN]{OFF} semgrep: 1 finding (low): services/backoffice/src/orders/export.ts:9 regex on user input\n[INFO] Report written to gl-sast-report.json\n", cmd("/analyzer run")),
        "e2e-checkout" => format!("{}\nRunning 12 tests using 4 workers\n\n  ✓  checkout › guest pays by card (8.1s)\n  ✓  checkout › signed-in customer keeps the cart (6.4s)\n  {YELLOW}↻{OFF}  checkout › 3-D Secure challenge (retry #1, 12.0s)\n  ✓  checkout › 3-D Secure challenge (9.8s)\n\n  {GREEN}12 passed{OFF} {YELLOW}(1 flaky){OFF} (1.2m)\n", cmd("npx playwright test e2e/checkout")),
        "compile" => format!("{}{}\nservices/catalog-api      built in 4.2s\nservices/orders-api       built in 5.0s\nservices/backoffice       built in 7.9s\n", cmd("npm ci --prefer-offline"), cmd("npm run build --workspaces")),
        "webapp-bundle" => format!("{}\n  ▲ Next.js 14.2.10\n   Creating an optimized production build ...\n ✓ Compiled successfully\n ✓ Generating static pages (42/42)\n\nRoute (app)                    Size     First Load JS\n┌ ○ /                          5.1 kB         98 kB\n├ ● /p/[slug]                  3.8 kB         96 kB\n└ ○ /checkout                  9.4 kB        112 kB\n", cmd("npm run build -w services/storefront-webapp")),
        "docker-image" => format!("{}\nINFO[0002] Retrieving image manifest node:22-alpine\nINFO[0041] Taking snapshot of full filesystem...\nINFO[0058] Pushing image to registry.example.org/shop/orders-api:release-2026-10-4f2a9c1e\nINFO[0063] Pushed registry.example.org/shop/orders-api@sha256:9b1f…e07a\n", cmd("/kaniko/executor --context services/orders-api --destination $IMAGE")),
        "deploy-review" | "deploy" => format!("{}\nRelease \"shop-review\" has been upgraded. Happy Helming!\nNAME: shop-review\nSTATUS: deployed\nREVISION: 14\n\nReview app: https://review.shop.example.org\n", cmd("helm upgrade --install shop-review charts/shop --namespace review --set image.tag=$CI_COMMIT_SHORT_SHA --wait")),
        "build" => format!("{}\nINFO[0063] Pushed registry.example.org/shop/orders-api:rc-1.8.0\n", cmd("/kaniko/executor --context services/orders-api --destination $IMAGE")),
        n if n.starts_with("helm-diff") => format!("{}\nshop, orders-api, Deployment (apps) has changed:\n   spec:\n     containers:\n{RED}-       image: registry.example.org/shop/orders-api:rc-1.7.3{OFF}\n{GREEN}+       image: registry.example.org/shop/orders-api:rc-1.8.0{OFF}\n         resources:\n           limits:\n{RED}-             memory: 512Mi{OFF}\n{GREEN}+             memory: 768Mi{OFF}\n", cmd("helm diff upgrade orders-api charts/orders-api -f charts/orders-api/values-preprod.yaml --set image.tag=rc-1.8.0")),
        n if n.starts_with("helm-upgrade") => format!("{}\nRelease \"orders-api\" is being upgraded...\nwaiting for deployment \"orders-api\" rollout to finish: 1 of 2 updated replicas are available...\n", cmd("helm upgrade orders-api charts/orders-api -f charts/orders-api/values-preprod.yaml --set image.tag=rc-1.8.0 --wait --timeout 10m")),
        "renovate" => format!("{}\n INFO: Repository started (repository=example-org/shop)\n INFO: Dependency extraction complete (baseBranch=main) \"stats\": {{\"managers\": {{\"npm\": {{\"fileCount\": 6, \"depCount\": 214}}, \"dockerfile\": {{\"fileCount\": 4, \"depCount\": 4}}}}}}\n INFO: Branch updated (branch=renovate/axios-1.x)\n INFO: Dependency Dashboard updated (issue=12)\n INFO: Repository finished (durationMs=88412)\n", cmd("renovate")),
        _ => cmd("make ci"),
    };
    log.push_str(&body);
    match j.status.name() {
        "success" => log.push_str(&format!("\x1b[0K{GREEN}Cleaning up project directory and file based variables{OFF}\n{GREEN}Job succeeded{OFF}\n")),
        "failed" => log.push_str(&format!("\x1b[0K{GREEN}Cleaning up project directory and file based variables{OFF}\n{RED}ERROR: Job failed: command terminated with exit code 1{OFF}\n")),
        _ => {}
    }
    log
}

/* ---------- Tickets ---------- */

/// What a ticket's preview shows beyond the snapshot's line.
#[derive(Default)]
struct TicketContent {
    description: &'static str,
    tags: &'static [&'static str],
    checklist: &'static [(&'static str, bool)],
    subtasks: &'static [(&'static str, &'static str)],
    /// (author index, text, hours ago)
    comments: &'static [(usize, &'static str, i64)],
    estimate_hours: Option<u64>,
    due_days: Option<i64>,
}

fn ticket_content(n: u32) -> TicketContent {
    match n {
        412 => TicketContent {
            description: "## Context\n\nCustomers fill a cart on their phone and pay on their laptop: today the cart stays on the phone. **42 %** of the abandoned carts last month were from signed-in customers.\n\n## Expected\n\n- A signed-in customer finds their cart on any device.\n- A guest's cart is merged into the account's at sign-in.\n- Carts older than 30 days are emptied.\n\n## Out of scope\n\nSharing a cart with someone else.",
            tags: &["storefront", "orders-api"],
            checklist: &[("API: GET / PUT /cart", true), ("Migration cart_lines", true), ("Storefront: load and save", true), ("Merge at sign-in", true), ("Test of the merge", false), ("Expiry after 30 days", false)],
            subtasks: &[("Cart expiry job", "todo")],
            comments: &[(3, "Design for the merge message: \"We added the 2 items from your last visit\".", 120), (0, "MR is up: !1288", 30), (1, "Reviewing it this morning.", 4)],
            estimate_hours: Some(14),
            due_days: Some(3),
        },
        433 => TicketContent {
            description: "A customer's accountant noticed the VAT lines of invoice **INV-2026-08812** add up to 59.97 € while the total says 59.98 €.\n\nWe round the VAT once on the total; French rules accept it, but the lines then do not add up. Round **per line** instead.\n\n```\n3 × 16.66 € at 20 %  →  3.332 + 3.332 + 3.332 = 9.996 → 10.00 (today)\n                     →  3.33 + 3.33 + 3.33     = 9.99         (expected)\n```",
            tags: &["orders-api", "invoices"],
            checklist: &[("Round per line", true), ("Regenerate the 12 invoices of August", false)],
            comments: &[(4, "Support has 3 tickets about this, all B2B customers.", 30), (0, "Found it, MR !1291. One test still fails on float sums.", 1)],
            estimate_hours: Some(4),
            due_days: Some(1),
            ..Default::default()
        },
        418 => TicketContent {
            description: "The order history page takes **2.4 s** for customers with hundreds of orders (B2B). Load 20 orders at a time, newest first, with a *Load more* button.",
            tags: &["orders-api", "storefront", "performance"],
            checklist: &[("Cursor pagination in the API", true), ("Load more in the storefront", true), ("Index on (customer_id, created_at, id)", true)],
            comments: &[(2, "Watch out for orders imported in bulk: same created_at.", 30)],
            estimate_hours: Some(7),
            ..Default::default()
        },
        421 => TicketContent {
            description: "When our payment provider calls us and orders-api is deploying, the webhook is lost and the order stays *awaiting payment*.\n\nRetry failed webhooks with an exponential backoff (1, 4, 16, 64 min), then put them in a dead letter queue visible in the back office.",
            tags: &["payments-api"],
            checklist: &[("Backoff", true), ("Dead letter queue", true), ("Back office list", true)],
            comments: &[(2, "Merged, on staging. To validate: stop orders-api for 5 min during a test payment.", 180)],
            estimate_hours: Some(10),
            ..Default::default()
        },
        425 => TicketContent {
            description: "Category pages load 60 photos at once: LCP is 3.1 s on 4G. Load them lazily, keep the first row eager.",
            tags: &["storefront", "performance"],
            checklist: &[("Category pages", true), ("Search results", false), ("Check the LCP on 4G", false)],
            estimate_hours: Some(7),
            ..Default::default()
        },
        427 => TicketContent {
            description: "Finance exports orders by copy-pasting the back office table into Excel. Add an **Export CSV** button that exports the current filter.\n\nColumns: order, date, customer, items, total excl. VAT, VAT, total, status.",
            tags: &["backoffice"],
            comments: &[(4, "Can it open in Excel without the import wizard? Finance uses French Excel.", 3000), (2, "Yes: semicolons, decimal comma and a BOM.", 2900)],
            estimate_hours: Some(4),
            ..Default::default()
        },
        436 => TicketContent {
            description: "Send the invoice PDF by email once the payment is confirmed, instead of the customer downloading it from their account.",
            tags: &["orders-api"],
            ..Default::default()
        },
        437 => TicketContent {
            description: "**Steps**\n\n1. Empty cart, apply coupon `WELCOME10` → \"Coupon applied\".\n2. Add a product.\n3. The discount is not there.\n\nThe coupon is checked against an empty cart and dropped.",
            tags: &["storefront", "bug"],
            comments: &[(3, "Reproduced on staging, Chrome and Safari.", 40)],
            ..Default::default()
        },
        439 => TicketContent {
            description: "Since this morning's storefront release, the payment page is **blank on Safari 16** (iOS 16 and macOS Ventura): `structuredClone` is not defined in a dependency.\n\n~6 % of our checkouts.",
            tags: &["storefront", "hotfix", "payments"],
            comments: &[(4, "Sentry: 312 errors in 2 h, all Safari 16.", 25), (1, "Polyfill or pin the dependency back? Pinning is faster.", 10)],
            due_days: Some(0),
            ..Default::default()
        },
        441 => TicketContent {
            description: "Swiss and UK customers want to pay in CHF and GBP, at prices we set, not converted ones.\n\nSee the refinement note in `docs/refinement/SHOP-441-multi-currency.md`: price lists per currency, currency from the delivery country.",
            tags: &["refinement"],
            subtasks: &[("catalog-api: price lists per currency", "to estimate"), ("orders-api: currency on orders and invoices", "to estimate"), ("Storefront: country picker", "to estimate")],
            comments: &[(1, "Accounting needs to answer question 2 before we estimate.", 1500), (0, "Asked them, answer expected Thursday.", 600)],
            ..Default::default()
        },
        442 => TicketContent {
            description: "Customers request a return from their order page: pick the items and a reason, get a prepaid label. Support validates refunds from the back office.",
            tags: &["refinement"],
            checklist: &[("Carrier API for labels", false), ("Refund flow with payments-api", false)],
            ..Default::default()
        },
        444 => TicketContent { description: "Sell outside the euro zone: currencies, languages and local payment methods.", tags: &["epic"], ..Default::default() },
        447 => TicketContent { description: "Let customers handle returns and deliveries without writing to support: 35 % of support tickets today.", tags: &["epic"], ..Default::default() },
        452 => TicketContent {
            description: "The payment provider's webhook secret leaked in a support screenshot. Rotate it in the provider and in Vault, then restart payments-api.",
            tags: &["security", "ops"],
            checklist: &[("New secret in Vault", true), ("Provider updated", false), ("Old secret revoked", false)],
            ..Default::default()
        },
        _ => TicketContent::default(),
    }
}

/// Every ticket the demo shows, with the statuses of its board.
fn tickets(s: &Snapshot) -> Vec<(&Ticket, &[crate::state::StatusOption])> {
    let mut out = Vec::new();
    for sprint in [&s.sprint, &s.next_sprint].into_iter().flatten() {
        out.extend(sprint.tickets.iter().map(|t| (t, sprint.statuses.as_slice())));
    }
    for b in &s.refinement.boards {
        out.extend(b.tickets.iter().map(|t| (t, b.statuses.as_slice())));
    }
    for b in &s.pickup.boards {
        out.extend(b.tickets.iter().map(|t| (t, b.statuses.as_slice())));
    }
    if let Some(b) = &s.time_tasks {
        out.extend(b.tickets.iter().map(|t| (t, b.statuses.as_slice())));
    }
    for t in &s.outside_sprint.tickets {
        out.push((t, t.list.as_ref().and_then(|l| s.outside_sprint.list_statuses.get(&l.id)).map(Vec::as_slice).unwrap_or(&[])));
    }
    out
}

/// Tickets of past sprints, delivered: found only by a search (the palette's), as a tracker's history.
fn delivered(s: &Snapshot) -> Vec<Ticket> {
    let Some(model) = s.sprint.as_ref().and_then(|sp| sp.tickets.first()) else { return Vec::new() };
    [(301, "Coupon codes at checkout", 40), (287, "Invoice PDF in the customer's language", 62), (264, "Product search by SKU", 90), (251, "Stripe webhooks retried on timeout", 120)]
        .into_iter()
        .map(|(n, name, days): (u32, &str, i64)| Ticket {
            id: format!("SHOP-{n}"),
            task_id: format!("demo{n}"),
            name: name.into(),
            status: "done".into(),
            closed: true,
            parent: None,
            task_type: None,
            task_type_icon: None,
            description: Some(format!("{name}.\n\nDelivered in an earlier sprint.")),
            url: format!("{}/SHOP-{n}", super::CLICKUP),
            created_at: Some(ago(days * 1440 + 600)),
            updated_at: Some(ago(days * 1440)),
            waiting_on: Vec::new(),
            blocking: Vec::new(),
            ..model.clone()
        })
        .collect()
}

/// The tickets a search finds: the demo's, and those delivered in past sprints.
pub fn search_tickets(s: &Snapshot, query: &str) -> Vec<Ticket> {
    crate::tracker::found(tickets(s).into_iter().map(|(t, _)| t.clone()).chain(delivered(s)), query)
}

/// A ticket's content: `id` is `SHOP-412`, or an internal id (`demo412`, a subtask's `demo412-1`).
pub fn ticket(s: &Snapshot, id: &str) -> Result<TaskView> {
    let all = tickets(s);
    // A subtask: built from its parent's content.
    if let Some((parent, k)) = id.strip_prefix("demo").and_then(|r| r.split_once('-')) {
        let (p, statuses) = all.iter().find(|(t, _)| t.task_id == format!("demo{parent}")).with_context(|| format!("{id}: not in the demo"))?;
        let n: u32 = parent.parse()?;
        let k: usize = k.parse()?;
        let (name, status) = ticket_content(n).subtasks.get(k.wrapping_sub(1)).copied().with_context(|| format!("{id}: not in the demo"))?;
        let sub = Ticket { id: format!("{}-{k}", p.id), task_id: id.into(), name: name.into(), status: status.into(), closed: false, parent: Some(p.task_id.clone()), description: None, task_type: None, task_type_icon: None, ..(*p).clone() };
        return Ok(view(s, &sub, statuses, TicketContent { description: "A part of its parent ticket, estimated with it.", ..Default::default() }));
    }
    let old = delivered(s);
    let statuses_of_sprint = s.sprint.as_ref().map(|sp| sp.statuses.as_slice()).unwrap_or(&[]);
    let (t, statuses) = all
        .iter()
        .copied()
        .chain(old.iter().map(|t| (t, statuses_of_sprint)))
        .find(|(t, _)| t.id == id || t.task_id == id)
        .with_context(|| format!("{id}: not a ticket of the demo"))?;
    let n: u32 = t.id.trim_start_matches("SHOP-").parse().unwrap_or(0);
    Ok(view(s, t, statuses, ticket_content(n)))
}

fn view(s: &Snapshot, t: &Ticket, statuses: &[crate::state::StatusOption], c: TicketContent) -> TaskView {
    let team = Demo::team();
    let description = if c.description.is_empty() { t.description.clone().unwrap_or_default() } else { c.description.into() };
    // An EPIC's children are its subtasks.
    let mut subtasks: Vec<Subtask> = tickets(s)
        .into_iter()
        .filter(|(c, _)| c.parent.as_deref() == Some(&t.task_id))
        .map(|(c, _)| Subtask { task_id: c.task_id.clone(), id: c.id.clone(), name: c.name.clone(), status: c.status.clone(), closed: c.closed })
        .collect();
    subtasks.extend(c.subtasks.iter().enumerate().map(|(k, (name, status))| Subtask {
        task_id: format!("{}-{}", t.task_id, k + 1),
        id: format!("{}-{}", t.id, k + 1),
        name: (*name).into(),
        status: (*status).into(),
        closed: false,
    }));
    TaskView {
        fields: t.fields.clone(),
        task_id: t.task_id.clone(),
        id: t.id.clone(),
        name: t.name.clone(),
        task_type: t.task_type.clone(),
        task_type_icon: t.task_type_icon.clone(),
        status: t.status.clone(),
        status_color: statuses.iter().find(|o| o.status == t.status).and_then(|o| o.color.clone()),
        statuses: statuses.to_vec(),
        closed: t.closed,
        priority: t.priority,
        assignees: t.assignees.clone(),
        creator: Some(team[4].clone()),
        tags: c.tags.iter().map(|x| x.to_string()).collect(),
        list: Some(t.list.as_ref().map(|l| l.name.clone()).unwrap_or_else(|| "Shop".into())),
        parent: t.parent.clone(),
        url: t.url.clone(),
        description,
        created_at: t.created_at,
        updated_at: t.updated_at,
        due_at: c.due_days.map(|d| now() + Duration::days(d)),
        time_estimate: c.estimate_hours.map(|h| h * 3_600_000),
        time_spent: t.time_spent,
        checklists: if c.checklist.is_empty() {
            Vec::new()
        } else {
            vec![Checklist { name: "Done when".into(), items: c.checklist.iter().map(|(name, resolved)| ChecklistItem { name: (*name).into(), resolved: *resolved }).collect() }]
        },
        subtasks,
        comments: c
            .comments
            .iter()
            .enumerate()
            .map(|(i, (who, text, hours))| Comment { id: format!("{}-c{i}", t.task_id), author: Some(team[*who].clone()), text: (*text).into(), date: Some(ago(hours * 60)), replies: 0 })
            .collect(),
        attachments: Vec::new(),
    }
}

/* ---------- Pods ---------- */

fn workload<'a>(s: &'a Snapshot, context: &str, namespace: &str, name: &str) -> Result<&'a crate::state::Workload> {
    s.environments
        .iter()
        .flat_map(|e| e.envs.iter())
        .filter(|e| e.context == context && e.namespace == namespace)
        .flat_map(|e| e.workloads.iter())
        .find(|w| w.name == name)
        .with_context(|| format!("{name}: not a workload of the demo"))
}

pub fn pods(s: &Snapshot, context: &str, namespace: &str, deployment: &str) -> Result<Vec<PodInfo>> {
    let w = workload(s, context, namespace, deployment)?;
    Ok((0..w.pods)
        .map(|i| {
            let failing = i >= w.ready;
            PodInfo {
                name: format!("{deployment}-7d9f8c6b5-{}", ["x2k9q", "m4h7t", "p8w3z"][i as usize % 3]),
                status: if failing { w.waiting.clone().unwrap_or_else(|| "Pending".into()) } else { "Running".into() },
                ready: !failing,
                restarts: if failing { w.restarts } else { 0 },
                started_at: Some(ago(if failing { 2 } else { 600 })),
                containers: vec![deployment.into()],
                last_exit: if failing { w.last_exit.clone() } else { None },
            }
        })
        .collect())
}

/// A pod's usage over time: a Pods block's pod as Prism keeps it (6 h at most), an environment's
/// from Google Managed Prometheus; the worker that crashes climbs to its memory limit, again and again.
pub fn pod_usage_history(s: &Snapshot, context: &str, namespace: &str, pod: &str, minutes: u32) -> Result<UsageHistory> {
    let followed = s.pods.iter().find(|p| p.context == context && p.namespace == namespace && p.name == pod);
    let (source, cpu, mem, (cpu_limit, mem_limit), crashing) = match followed {
        Some(p) => {
            let now = s.pod_usage.get(&format!("{}/{namespace}/{pod}", p.target)).copied();
            let cpu = now.map_or(120, |u| u.cpu_millis.max(5));
            let mem = now.map_or(256 << 20, |u| u.mem_bytes.max(16 << 20));
            (HistorySource::Prism, cpu, mem, (p.cpu_limit_millis, p.mem_limit_bytes), p.last_exit.is_some())
        }
        None => {
            let name = pod.rsplitn(3, '-').nth(2).with_context(|| format!("{pod}: not a pod of the demo"))?;
            let w = workload(s, context, namespace, name)?;
            let n = u64::from(w.pods.max(1));
            let per = |v: Option<u64>| v.map(|v| v / n);
            let crashing = w.restarts > 0 && pod.ends_with("m4h7t");
            (HistorySource::Gcp, per(w.load.cpu_used_millis).unwrap_or(150).max(5), per(w.load.mem_used_bytes).unwrap_or(300 << 20).max(16 << 20), (per(w.load.cpu_limit_millis), per(w.load.mem_limit_bytes).or(Some(768 << 20))), crashing)
        }
    };
    let minutes = match source {
        HistorySource::Prism => minutes.min(6 * 60),
        HistorySource::Gcp => minutes,
    };
    let step: i64 = match source {
        HistorySource::Prism => 15.max(i64::from(minutes) * 60 / 1440),
        HistorySource::Gcp => 30.max(i64::from(minutes) * 60 / 240),
    };
    // A Prism-kept history starts when Prism did (the demo's launch, an hour back at most).
    let end = now().timestamp();
    let start = match source {
        HistorySource::Prism => (end - i64::from(minutes) * 60).max(end - 3600),
        HistorySource::Gcp => end - i64::from(minutes) * 60,
    };
    let seed = pod.bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    let points = (0..)
        .map(|i| start + i * step)
        .take_while(|at| *at <= end)
        .map(|at| {
            let x = at as f64;
            let wave = 1.0 + 0.25 * (x / 1700.0 + seed as f64).sin() + 0.1 * (x / 230.0 + (seed % 13) as f64).sin();
            // A burst each hour, and the morning's traffic on longer periods.
            let day = 1.0 + 0.35 * ((x / 86_400.0) * std::f64::consts::TAU).sin();
            let burst = if (at / 60) % 60 < 4 { 1.8 } else { 1.0 };
            let cpu = (cpu as f64 * wave * day * burst).round() as u64;
            let mem = match (crashing, mem_limit) {
                // Climbing to its limit in 40 minutes, then killed and started again.
                (true, Some(limit)) => (limit as f64 * (0.45 + 0.55 * ((at % 2400) as f64 / 2400.0))).round() as u64,
                _ => (mem as f64 * (1.0 + 0.06 * (x / 3100.0 + seed as f64).sin()) * day.sqrt()).round() as u64,
            };
            UsagePoint { at, cpu_millis: Some(cpu), mem_bytes: Some(mem) }
        })
        .collect();
    Ok(UsageHistory { source, points, cpu_limit_millis: cpu_limit, mem_limit_bytes: mem_limit, step: step as u32, error: None })
}

pub fn pod_log(s: &Snapshot, context: &str, namespace: &str, pod: &str, previous: bool) -> Result<PodLog> {
    // A pod of the Pods block: the worker that crashes, or a few quiet lines.
    if let Some(p) = s.pods.iter().find(|p| p.context == context && p.namespace == namespace && p.name == pod) {
        let line = |minutes: i64, level: &str, msg: &str| format!("{} {level:<5} {msg}\n", ago(minutes).format("%Y-%m-%dT%H:%M:%S%.3fZ"));
        let mut log = line(5, "INFO", &format!("{} starting", p.workload.as_deref().unwrap_or(pod)));
        if p.last_exit.is_some() {
            log += &line(5, "INFO", "connecting to redis://queue:6379");
            if previous {
                log += &line(4, "ERROR", "connect ECONNREFUSED 10.0.12.7:6379");
                log += &line(4, "FATAL", "queue unreachable, exiting (code 1)");
            }
        } else {
            log += &line(4, "INFO", "ready");
        }
        return Ok(PodLog { log, truncated: false });
    }
    let name = pod.rsplitn(3, '-').nth(2).with_context(|| format!("{pod}: not a pod of the demo"))?;
    let w = workload(s, context, namespace, name)?;
    let tag = w.image_tag.clone().unwrap_or_default();
    let line = |minutes: i64, level: &str, msg: &str| format!("{} {level:<5} {msg}\n", ago(minutes).format("%Y-%m-%dT%H:%M:%S%.3fZ"));
    let mut log = line(600, "INFO", &format!("{name} {tag} starting (node v22.11.0, NODE_ENV=production)"));
    log += &line(600, "INFO", "connected to postgres (pool 10)");
    log += &line(600, "INFO", "listening on :8080");
    let crashing = w.restarts > 0 && pod.ends_with("m4h7t");
    if crashing {
        log = line(if previous { 6 } else { 2 }, "INFO", &format!("{name} {tag} starting (node v22.11.0, NODE_ENV=production)"));
        log += &line(if previous { 6 } else { 2 }, "INFO", "connected to postgres (pool 10)");
        log += &line(if previous { 6 } else { 2 }, "INFO", "warming the cart cache: 48 211 carts");
        if previous {
            log += &line(5, "WARN", "heap used 701 MiB / 768 MiB");
            log += "\n<--- Last few GCs --->\n[1:0x7f3a2c0]   61234 ms: Mark-Compact 702.1 (742.8) -> 698.4 (741.3) MB, 812.33 / 0.00 ms  (average mu = 0.112, current mu = 0.031) allocation failure\n\nFATAL ERROR: Reached heap limit Allocation failed - JavaScript heap out of memory\n----- Native stack trace -----\n 1: 0xb8d0a3 node::OOMErrorHandler(char const*, v8::OOMDetails const&)\n";
        }
        return Ok(PodLog { log, truncated: false });
    }
    let routes = if name.contains("worker") {
        vec!["job invoice.render order=ORD-58812 done in 840ms", "job webhook.deliver attempt=2 status=200", "job cart.expire removed=31"]
    } else if name.contains("catalog") {
        vec!["GET /products?category=shoes&page=2 200 18ms", "GET /search?q=SKU-4410 200 9ms", "GET /products/trail-runner-2 200 6ms"]
    } else if name.contains("payments") {
        vec!["POST /webhooks/provider 200 22ms", "POST /payments 201 310ms", "GET /payments/pay_8f2c 200 5ms"]
    } else {
        vec!["GET /orders?after=c_17282 200 41ms", "PUT /cart 204 12ms", "POST /orders 201 96ms", "GET /invoices/INV-2026-09931.pdf 200 230ms"]
    };
    for i in 0..40 {
        log += &line(590 - i * 14, "INFO", routes[i as usize % routes.len()]);
        if i % 13 == 7 {
            log += &line(590 - i * 14, "WARN", "slow query (412ms): SELECT * FROM orders WHERE customer_id = $1 ORDER BY created_at DESC");
        }
    }
    Ok(PodLog { log, truncated: false })
}

/* ---------- Agenda ---------- */

/// A day's events: the stand-up every working day, and meetings that depend on the weekday.
/// What the pages wizard finds on the demo's machine: glab signed in, Claude Code and Codex,
/// the shop's clones, its three contexts. Nothing of this machine is probed.
pub fn detection() -> prism_core::detect::Detection {
    use prism_core::agents::provider::Provider as Cli;
    use prism_core::detect::{AgentFound, Detection, ForgeFound, RepoFound, TrackersFound};
    use prism_core::forge::Provider as Forge;
    let host = HOST.trim_start_matches("https://");
    let forge = |provider, installed, host: &str, login: Option<&str>| ForgeFound { provider, installed, host: host.into(), login: login.map(Into::into) };
    let agent = |provider, found: Option<(&str, &str)>| AgentFound { provider, path: found.map(|(p, _)| p.into()), version: found.map(|(_, v)| v.into()) };
    let repo = |name: &str, project: &str| RepoFound {
        name: name.into(),
        path: format!("~/Development/{name}"),
        root: "~/Development".into(),
        project: Some(project.into()),
        host: Some(host.into()),
        forge: Some(Forge::Gitlab),
        touched: now().timestamp(),
    };
    Detection {
        forges: vec![forge(Forge::Gitlab, true, host, Some("alex")), forge(Forge::Github, true, "github.com", None)],
        agents: Cli::ALL
            .into_iter()
            .map(|p| match p {
                Cli::Claude => agent(p, Some(("~/.local/bin/claude", "2.1.3 (Claude Code)"))),
                Cli::Codex => agent(p, Some(("/opt/homebrew/bin/codex", "codex-cli 0.46.0"))),
                _ => agent(p, None),
            })
            .collect(),
        tmux: Some("tmux 3.5a".into()),
        folders: vec!["~/Development".into()],
        repos: vec![repo("shop", PROJECT), repo("helm", HELM)],
        kubectl: true,
        kube_contexts: vec!["demo-prod".into(), "demo-preprod".into(), "demo-staging".into()],
        trackers: TrackersFound { clickup_env: true, jira_env: false, obsidian_vaults: Vec::new() },
    }
}

pub fn day(day: NaiveDate) -> Vec<CalendarEvent> {
    let at = |h: u32, m: u32| day.and_hms_opt(h, m, 0).and_then(|t| t.and_local_timezone(chrono::Local).single()).map(|t| t.with_timezone(&Utc)).unwrap_or_else(now);
    let event = |key: &str, title: &str, start: DateTime<Utc>, minutes: i64, attendees: u32| CalendarEvent {
        id: format!("{key}@{day}"),
        title: title.into(),
        start,
        end: start + Duration::minutes(minutes),
        all_day: false,
        location: None,
        calendar: "Work".into(),
        my_status: Some(if key == "review" && day > now().date_naive() { "pending" } else { "accepted" }.into()),
        canceled: false,
        free: false,
        join_url: Some("https://meet.example.org/shop-team".into()),
        attendees,
    };
    let weekday = day.weekday().num_days_from_monday();
    if weekday >= 5 {
        return Vec::new();
    }
    let mut out = vec![event("standup", "Daily stand-up", at(9, 30), 15, 6)];
    match weekday {
        0 => out.push(event("planning", "Sprint planning", at(14, 0), 90, 7)),
        1 => out.push(event("archi", "Architecture review: order events", at(15, 0), 60, 4)),
        2 => out.push(event("lunch", "Team lunch", at(12, 30), 75, 6)),
        _ => {}
    }
    out.push(event("one", "1:1 with Sam", at(11, 0), 30, 2));
    if weekday == 3 || weekday == 4 {
        out.push(event("refinement", "Refinement: multi-currency", at(14, 0), 60, 5));
        out.push(event("review", "Sprint review", at(16, 30), 45, 9));
    }
    out.sort_by_key(|e| e.start);
    out
}

pub fn event(id: &str) -> Result<EventDetails> {
    let (_, date) = id.split_once('@').context("not an event of the demo")?;
    let date: NaiveDate = date.parse().context("not an event of the demo")?;
    let e = day(date).into_iter().find(|e| e.id == id).context("not an event of the demo")?;
    let team = Demo::team();
    let attendee = |i: usize, status: &str| Attendee {
        name: team[i].name.clone(),
        email: Some(format!("{}@example.org", login(i).0)),
        status: status.into(),
        optional: false,
        me: i == 0,
    };
    let key = id.split('@').next().unwrap_or_default();
    let (attendees, notes): (Vec<Attendee>, &str) = match key {
        "one" => (vec![attendee(0, "accepted"), attendee(1, "accepted")], "Sam's topics: the pagination review, the on-call rota for November."),
        "refinement" => (
            vec![attendee(0, "accepted"), attendee(1, "accepted"), attendee(2, "tentative"), attendee(3, "accepted"), attendee(4, "accepted")],
            "Agenda\n\n1. SHOP-441 multi-currency prices: the open questions\n2. SHOP-442 returns portal: carrier API options\n3. Estimates\n\nRead docs/refinement/SHOP-441-multi-currency.md before.",
        ),
        "review" => ((0..5).map(|i| attendee(i, if i == 2 { "declined" } else { "accepted" })).collect(), "Demo of the sprint's tickets on staging: saved cart, CSV export, webhooks retry."),
        "planning" => ((0..5).map(|i| attendee(i, "accepted")).collect(), "Bring your estimates for Sprint 43 (ClickUp board)."),
        _ => ((0..5).map(|i| attendee(i, "accepted")).collect(), "Yesterday, today, blockers. 15 minutes."),
    };
    Ok(EventDetails {
        title: e.title,
        start: e.start,
        end: e.end,
        all_day: false,
        location: Some("Room 2 · and online".into()),
        calendar: e.calendar,
        organizer: Some(Attendee { me: false, ..attendee(4, "accepted") }),
        attendees,
        notes: Some(notes.into()),
        url: None,
        join_url: e.join_url,
        my_status: e.my_status,
        canceled: false,
    })
}

/* ---------- Agents' folders ---------- */

/// The repository's files, as an agent's worktree lists them.
const TREE: &[&str] = &[
    ".gitlab-ci.yml",
    "CHANGELOG.md",
    "README.md",
    "docs/architecture.md",
    "package.json",
    "services/backoffice/src/orders/OrderList.tsx",
    "services/backoffice/src/orders/export.ts",
    "services/catalog-api/package.json",
    "services/catalog-api/src/search.ts",
    "services/orders-api/migrations/2026_10_06_cart_lines.sql",
    "services/orders-api/src/cart/cart.repository.ts",
    "services/orders-api/src/invoices/pdf.ts",
    "services/orders-api/src/invoices/vat.test.ts",
    "services/orders-api/src/invoices/vat.ts",
    "services/orders-api/src/orders/orders.repository.ts",
    "services/orders-api/src/routes.ts",
    "services/payments-api/webhooks/retry.go",
    "services/storefront-webapp/src/cart/useCart.ts",
    "services/storefront-webapp/src/catalog/ProductCard.tsx",
];

/// What each demo agent changed: (path, status), and the MR its branch became.
type Work = (Vec<(&'static str, &'static str)>, Option<u64>);

fn agent_work(id: &str) -> Result<Work> {
    Ok(match id {
        "demo-a1" => (vec![("services/orders-api/src/invoices/vat.ts", "M"), ("services/orders-api/src/invoices/vat.test.ts", "M")], Some(1291)),
        "demo-a2" => (Vec::new(), Some(1288)),
        "demo-a3" => (vec![("docs/refinement/SHOP-441-multi-currency.md", "?")], None),
        _ => bail!("{id}: not an agent of the demo"),
    })
}

fn agent_root(s: &Snapshot, id: &str) -> Result<String> {
    s.agents.iter().find(|a| a.id == id).and_then(|a| a.worktree.clone()).with_context(|| format!("{id}: not an agent of the demo"))
}

pub fn agent_files(s: &Snapshot, id: &str) -> Result<Listing> {
    let root = agent_root(s, id)?;
    let (changes, _) = agent_work(id)?;
    let mut files: Vec<FileEntry> = TREE.iter().map(|p| FileEntry { path: (*p).into(), status: changes.iter().find(|(c, _)| c == p).map(|(_, st)| (*st).into()), ignored: false }).collect();
    for (p, st) in &changes {
        if !TREE.contains(p) {
            files.push(FileEntry { path: (*p).into(), status: Some((*st).into()), ignored: false });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Listing { root, files, truncated: false })
}

pub fn agent_diff(s: &Snapshot, id: &str, branch: bool) -> Result<Diff> {
    agent_root(s, id)?;
    let (changes, mr) = agent_work(id)?;
    let mut files: Vec<FileDiff> = match mr {
        // Uncommitted: what the agent is changing now; the branch: all its MR brings.
        Some(iid) if branch || !changes.is_empty() => mr_content(None, iid)?.files.into_iter().filter(|f| branch || changes.iter().any(|(p, _)| *p == f.new_path)).collect(),
        _ => Vec::new(),
    };
    if mr.is_none() {
        files.extend(changes.iter().map(|(p, _)| added(p)));
    }
    Ok(Diff { files, truncated: false, against: if branch { "origin/release/2026.10".into() } else { "HEAD".into() }, head: "demo".into() })
}

/// A file of an agent's folder: the demo's own text, or a stand-in.
pub fn agent_text(s: &Snapshot, id: &str, path: &str) -> Result<String> {
    agent_root(s, id)?;
    if !TREE.contains(&path) && !agent_work(id)?.0.iter().any(|(p, _)| *p == path) {
        bail!("{path}: not in the agent's folder");
    }
    Ok(file(path).map(String::from).unwrap_or_else(|| stand_in(path)))
}

fn stand_in(path: &str) -> String {
    let comment = if path.ends_with(".go") || path.ends_with(".ts") || path.ends_with(".tsx") { "//" } else { "#" };
    format!("{comment} {path}\n{comment} A sample of the demo profile: this file's content is not part of it.\n")
}

pub fn agent_preview(s: &Snapshot, id: &str, path: &str) -> Result<Preview> {
    let text = agent_text(s, id, path)?;
    let (changes, _) = agent_work(id)?;
    // Changed: lines 12 to 15 are the agent's, as committed they were not there.
    let (head, new) = match changes.iter().find(|(p, _)| *p == path) {
        Some((_, "?" | "A")) => (None, true),
        Some(_) => (Some(text.lines().enumerate().filter(|(i, _)| !(11..15).contains(i)).map(|(_, l)| format!("{l}\n")).collect::<String>()).filter(|h| *h != text), false),
        None => (None, false),
    };
    Ok(Preview::Text { size: text.len() as u64, text, head, new, stamp: "demo".into() })
}

/// The Files search over the demo's files, plain text (a regex read as such).
pub fn agent_search(s: &Snapshot, id: &str, q: &Query) -> Result<Found> {
    agent_root(s, id)?;
    let (changes, _) = agent_work(id)?;
    let mut paths: Vec<&str> = TREE.iter().copied().chain(changes.iter().map(|(p, _)| *p)).collect();
    paths.sort();
    paths.dedup();
    let needle = if q.case { q.text.clone() } else { q.text.to_lowercase() };
    let mut matches = Vec::new();
    for p in paths.into_iter().filter(|p| q.paths.as_ref().is_none_or(|only| only.iter().any(|o| o == p))) {
        let text = agent_text(s, id, p)?;
        for (i, l) in text.lines().enumerate() {
            let hay = if q.case { l.to_string() } else { l.to_lowercase() };
            if !needle.is_empty() && hay.contains(&needle) {
                if matches.len() == q.max as usize {
                    return Ok(Found { matches, truncated: true });
                }
                matches.push(Match { path: p.into(), line: i as u32 + 1, text: l.into() });
            }
        }
    }
    Ok(Found { matches, truncated: false })
}

/* ---------- Agents' terminals ---------- */

/// What an agent's terminal shows: a recorded Claude Code session, for the terminal to replay.
pub fn terminal(s: &Snapshot, id: &str) -> Result<Vec<u8>> {
    let a = s.agents.iter().find(|a| a.id == id).with_context(|| format!("{id}: not an agent of the demo"))?;
    const O: &str = "\x1b[38;5;173m"; // Claude's orange
    const B: &str = "\x1b[1m";
    const G: &str = "\x1b[32m";
    const R: &str = "\x1b[31m";
    const D: &str = "\x1b[2m";
    const X: &str = "\x1b[0m";
    let header = format!(
        "{O}╭──────────────────────────────────────────────────────╮{X}\n{O}│{X} {O}✻{X} {B}Claude Code{X}                                         {O}│{X}\n{O}│{X}   {D}cwd: {}{X}\n{O}╰──────────────────────────────────────────────────────╯{X}\n\n{D}>{X} {}\n\n",
        a.worktree.as_deref().unwrap_or(&a.cwd),
        a.prompt.as_deref().unwrap_or(&a.title)
    );
    let tool = |name: &str, arg: &str, out: &str| format!("{G}●{X} {B}{name}{X}({arg})\n  {D}⎿  {out}{X}\n\n");
    let say = |text: &str| format!("{B}●{X} {text}\n\n");
    let body = match id {
        "demo-a1" => [
            say("I'll read how invoices compute their VAT, and the tests around it."),
            tool("Read", "services/orders-api/src/invoices/vat.ts", "Read 21 lines"),
            tool("Read", "services/orders-api/src/invoices/vat.test.ts", "Read 33 lines"),
            say("The VAT is summed over all lines, then rounded once: the printed lines can be a cent off the total. I'll round each line, half away from zero, and add two tests."),
            tool("Update", "services/orders-api/src/invoices/vat.ts", &format!("Updated with {G}5 additions{X}{D} and {R}2 removals{X}")),
            tool("Update", "services/orders-api/src/invoices/vat.test.ts", &format!("Updated with {G}10 additions{X}{D} and {R}1 removal{X}")),
            format!("{O}╭──────────────────────────────────────────────────────╮{X}\n{O}│{X} {B}Bash command{X}\n{O}│{X}\n{O}│{X}   npm run test:invoices\n{O}│{X}   {D}Run the invoice tests{X}\n{O}│{X}\n{O}│{X} Do you want to proceed?\n{O}│{X} {O}❯{X} 1. Yes\n{O}│{X}   2. Yes, and don't ask again for {B}npm run test:invoices{X}\n{O}│{X}   3. No, and tell Claude what to do differently {D}(esc){X}\n{O}╰──────────────────────────────────────────────────────╯{X}\n"),
        ]
        .concat(),
        "demo-a2" => [
            say("Two review threads are open on !1288. I'll address them."),
            tool("Read", "services/orders-api/src/cart/cart.repository.ts", "Read 24 lines"),
            tool("Update", "services/orders-api/src/cart/cart.repository.ts", &format!("Updated with {G}6 additions{X}{D} and {R}4 removals{X}")),
            tool("Write", "services/orders-api/src/cart/cart.merge.test.ts", "Wrote 38 lines"),
            tool("Bash", "npm run test -w services/orders-api -- cart", "✓ 14 tests passed (1.2s)"),
            say("Done, ready for your review:\n\n  • Upsert on (customer_id, sku): a save keeps each line's added_at, so the cart's order no longer changes.\n  • Merge at sign-in is tested: the same SKU in both carts adds the quantities.\n\n  Nothing is pushed yet: say push and I'll push and reply on both threads."),
            format!("{D}──────────────────────────────────────────────────────{X}\n{B}>{X} \n"),
        ]
        .concat(),
        _ => [
            say("I'll map what multi-currency touches before writing the refinement note."),
            tool("Grep", "\"price|amount|currency\" in services/", "Found 184 matches in 41 files"),
            tool("Read", "services/catalog-api/src/prices.ts", "Read 88 lines"),
            tool("Write", "docs/refinement/SHOP-441-multi-currency.md", "Wrote 38 lines"),
            say("The note is in docs/refinement/SHOP-441-multi-currency.md: price lists per currency, the currency from the delivery country, an impact table by component (about 7 days), and two questions for accounting."),
            format!("{D}Session ended.{X}\n"),
        ]
        .concat(),
    };
    Ok(format!("{header}{body}").replace('\n', "\r\n").into_bytes())
}

/// A ticket's commits: two merged on main, two on its branch still open.
pub fn ticket_commits(id: &str) -> Vec<crate::collectors::ticket_commits::TicketCommit> {
    use crate::collectors::ticket_commits::TicketCommit;
    let now = Utc::now();
    let key = id.to_lowercase();
    let commit = |n: u8, subject: &str, author: &str, hours: i64, merged: bool| {
        let sha = format!("{:02x}{}", n, "3fa9c1d7e28b4a6f90c5d1e7a3b2f4c6d8e0a1b3".get(..38).unwrap_or_default());
        TicketCommit {
            repo: "shop".into(),
            short: sha[..8].into(),
            url: Some(format!("{HOST}/{PROJECT}/-/commit/{sha}")),
            sha,
            subject: subject.into(),
            author: author.into(),
            date: now - Duration::hours(hours),
            merged,
            branch: (!merged).then(|| format!("feat/{key}-vat-rates")),
        }
    };
    vec![
        commit(4, "test: VAT rounding on mixed baskets", "Léa Martin", 2, false),
        commit(3, "feat: per-country VAT rates", "Léa Martin", 5, false),
        commit(2, &format!("fix: {id} the invoice total ignores the discount"), "Hugo Bernard", 30, true),
        commit(1, &format!("feat: {id} invoice VAT lines (!1793)"), "Léa Martin", 52, true),
    ]
}

/// An agent's context detail: a session of an hour on the estimates, scaled to its gauge.
pub fn breakdown(context: crate::agents::context::ContextUse) -> crate::agents::breakdown::Breakdown {
    use crate::agents::breakdown::{Breakdown, Heavy, Part, ToolShare};
    let base = 24_100.min(context.used);
    let rest = (context.used - base) as f64;
    let n = |share: f64| (rest * share).round() as u64;
    let tool = |name: &str, share: f64, count: u32| ToolShare { tool: name.into(), tokens: n(share), count };
    let heavy = |tool: &str, what: &str, share: f64, turn: u32, call: bool| Heavy { tool: tool.into(), what: Some(what.into()), tokens: n(share), turn, call };
    Breakdown {
        used: context.used,
        size: context.size,
        base: Some(base),
        parts: vec![
            Part { kind: "prompts", tokens: n(0.02), tools: vec![] },
            Part { kind: "replies", tokens: n(0.08), tools: vec![] },
            Part { kind: "calls", tokens: n(0.04), tools: vec![tool("Edit", 0.025, 9), tool("Bash", 0.01, 17), tool("Read", 0.005, 23)] },
            Part { kind: "results", tokens: n(0.81), tools: vec![tool("Read", 0.49, 23), tool("Bash", 0.18, 17), tool("Grep", 0.07, 11), tool("WebFetch", 0.05, 2), tool("Agent", 0.02, 1)] },
            Part { kind: "images", tokens: n(0.03), tools: vec![] },
            Part { kind: "reminders", tokens: n(0.02), tools: vec![] },
        ],
        heaviest: vec![
            heavy("Read", "src/invoices/vat.rs", 0.083, 4, false),
            heavy("Bash", "cargo test -p billing", 0.06, 11, false),
            heavy("Read", "src/invoices/mod.rs", 0.058, 4, false),
            heavy("WebFetch", "https://docs.rs/rust_decimal/latest/rust_decimal/", 0.04, 7, false),
            heavy("Read", "tests/fixtures/invoices.json", 0.039, 9, false),
            heavy("Edit", "src/invoices/vat.rs", 0.012, 12, true),
        ],
        compactions: 0,
        compacted_at: None,
    }
}

/// An agent's Activity tab: the VAT agent's last 52 minutes, two turns (a short one done, then one
/// going in circles at the end); the others a short run.
pub fn activity(id: &str) -> crate::agents::activity::Activity {
    use crate::agents::hook::Line;
    let now = chrono::Utc::now().timestamp_millis();
    let at = |minutes: f64| now - (minutes * 60_000.0) as i64;
    let mut lines = Vec::new();
    let mut n = 0;
    let mut tool = |lines: &mut Vec<Line>, start: f64, secs: f64, name: &str, path: Option<&str>, cmd: Option<&str>, failed: bool| {
        n += 1;
        let id = Some(format!("toolu_demo_{n}"));
        let base = Line { tool: Some(name.into()), id, path: path.map(String::from), cmd: cmd.map(String::from), what: path.map(|p| p.rsplit('/').next().unwrap_or(p).to_string()), ..Default::default() };
        lines.push(Line { t: at(start), event: "PreToolUse".into(), ..base.clone() });
        lines.push(Line { t: at(start) + (secs * 1000.0) as i64, event: if failed { "PostToolUseFailure" } else { "PostToolUse" }.into(), ..base });
    };
    let vat = "services/orders-api/src/invoices/vat.ts";
    let test = "services/orders-api/src/invoices/vat.test.ts";
    let credit = "services/orders-api/src/invoices/credit-note.ts";
    if id == "demo-a1" {
        lines.push(Line { t: at(52.0), event: "UserPromptSubmit".into(), ..Default::default() });
        tool(&mut lines, 51.5, 1.0, "Read", Some(vat), None, false);
        tool(&mut lines, 50.0, 2.0, "Grep", Some("services/orders-api/src/invoices"), None, false);
        tool(&mut lines, 48.0, 11.0, "Bash", None, Some("npm run test:invoices"), false);
        lines.push(Line { t: at(46.5), event: "Stop".into(), ..Default::default() });
        lines.push(Line { t: at(38.0), event: "UserPromptSubmit".into(), ..Default::default() });
        for (i, f) in ["services/orders-api/src/invoices/index.ts", vat, test, "services/orders-api/src/invoices/total.ts", "services/orders-api/src/money.ts", credit].iter().enumerate() {
            tool(&mut lines, 37.5 - i as f64 * 0.6, 1.0, "Read", Some(f), None, false);
        }
        tool(&mut lines, 33.0, 2.0, "Grep", Some("services/orders-api/src"), None, false);
        tool(&mut lines, 31.0, 3.0, "Edit", Some(test), None, false);
        lines.push(Line { t: at(30.0), event: "PermissionRequest".into(), tool: Some("Bash".into()), cmd: Some("npm run test:invoices".into()), what: Some("Run the invoice tests".into()), ..Default::default() });
        tool(&mut lines, 27.0, 9.0, "Bash", None, Some("npm run test:invoices"), true);
        tool(&mut lines, 25.0, 4.0, "Edit", Some(vat), None, false);
        tool(&mut lines, 23.5, 8.0, "Bash", None, Some("npm run test:invoices"), false);
        tool(&mut lines, 21.0, 1.0, "Read", Some(credit), None, false);
        tool(&mut lines, 19.0, 3.0, "Edit", Some(credit), None, false);
        for m in [16.0, 12.5, 9.0, 5.5] {
            tool(&mut lines, m, 9.0, "Bash", None, Some("npm run test:invoices"), true);
            tool(&mut lines, m - 1.5, 1.0, "Read", Some(credit), None, false);
        }
        lines.push(Line { t: at(2.5), event: "PermissionRequest".into(), tool: Some("Bash".into()), cmd: Some("npm run test:invoices -- --verbose".into()), what: Some("Run the invoice tests, verbose".into()), ..Default::default() });
    } else {
        lines.push(Line { t: at(60.0), event: "UserPromptSubmit".into(), ..Default::default() });
        tool(&mut lines, 59.0, 1.0, "Read", Some("services/orders-api/src/cart/cart.repository.ts"), None, false);
        tool(&mut lines, 56.0, 3.0, "Edit", Some("services/orders-api/src/cart/cart.repository.ts"), None, false);
        tool(&mut lines, 52.0, 14.0, "Bash", None, Some("npm test -- cart"), false);
        lines.push(Line { t: at(50.0), event: "Stop".into(), ..Default::default() });
    }
    lines.sort_by_key(|l| l.t);
    crate::agents::activity::fold(&lines)
}

/// An agent's conversation on a phone (its Chat tab): the VAT agent's two turns, the second still
/// going; the others a short one.
/// A question a demo agent asks, still unanswered.
fn ask() -> crate::agents::question::Ask {
    use crate::agents::question::{Ask, Choice, Question};
    let choice = |label: &str, description: &str| Choice { label: label.into(), description: description.into() };
    Ask {
        call: "toolu_demo_ask".into(),
        questions: vec![
            Question { header: "Start".into(), question: "Which part first?".into(), multi: false, options: vec![choice("Backend", "The endpoint and its tests"), choice("Front", "The button, behind the flag")] },
            Question { header: "Checks".into(), question: "What should I run before the MR?".into(), multi: true, options: vec![choice("Unit tests", ""), choice("Lint", ""), choice("E2E", "Slow: about 6 minutes")] },
        ],
        answers: vec![None, None],
    }
}

pub fn conversation(id: &str) -> crate::agents::conversation::Conversation {
    use crate::agents::conversation::{Background, Conversation, Entry, PlanStep, Sub, SubStep, Tool, Turn};
    use crate::agents::live::Live;
    let iso = |minutes: i64| Some((now() - Duration::minutes(minutes)).to_rfc3339());
    let text = |t: &str| Entry::Text { text: t.into() };
    let tool = |name: &str, kind: &'static str, what: &str, state: &'static str| Tool { tool: name.into(), kind, what: Some(what.into()), state, ms: (state != "running").then_some(1300), ..Tool::default() };
    let thought = |text: &str, ms: u64| Entry::Thinking { text: text.into(), ms: Some(ms) };
    let vat = "services/orders-api/src/invoices/vat.ts";
    let credit = "services/orders-api/src/invoices/credit-note.ts";
    let tests = "npm run test:invoices";
    let edit = |path: &str, diff: &[&str]| {
        let mut t = tool("Edit", "write", path, "ok");
        t.diff = diff.iter().map(|l| l.to_string()).collect();
        t.added = diff.iter().filter(|l| l.starts_with('+')).count() as u32;
        t.removed = diff.iter().filter(|l| l.starts_with('-')).count() as u32;
        Entry::Tool(t)
    };
    let run = |state: &'static str, output: &str| Entry::Tool(Tool { output: Some(output.into()), ..tool("Bash", "command", tests, state) });
    if id == "demo-a2" {
        // Plan mode: a first plan sent back with your words, the second one waiting.
        use crate::agents::plan::Proposal;
        let v1 = "# Multi-currency prices\n\n## Context\nPrices are stored in EUR only (`products.price_cents`); the storefront converts on the fly with a daily rate.\n\n## Steps\n1. Add a `prices` table: product, currency, amount in cents.\n2. Build the rows with `pandas` from the current EUR prices and the rates.\n3. Read the price in the visitor's currency in `ProductCard.tsx`, EUR as fallback.\n4. Write tests for the price lookup.\n\n## Out of scope\nTaxes per country, rounding rules per currency.";
        let v2 = v1.replace("2. Build the rows with `pandas` from the current EUR prices and the rates.", "2. Fill the rows with a SQL migration from the current EUR prices and the rates, no new dependency.");
        let plan = |call: &str, text: &str, version, verdict, feedback: Option<&str>| Entry::Tool(Tool { proposal: Some(Proposal { call: call.into(), text: text.into(), version, verdict, feedback: feedback.map(String::from), file: None }), ..tool("ExitPlanMode", "other", "", if verdict == "waiting" { "running" } else { "failed" }) });
        return Conversation {
            turns: vec![Turn { n: 1, prompt: "Plan multi-currency prices for SHOP-441, nothing else.".into(), at: iso(30), last: iso(2), entries: vec![
                Entry::Tool(tool("Read", "read", "services/catalog/src/prices.ts", "ok")),
                Entry::Tool(tool("Grep", "read", "price_cents", "ok")),
                Entry::Tool(tool("Write", "write", "~/.claude/plans/multi-currency-prices.md", "ok")),
                plan("p1", v1, 1, "changes", Some("Comments on your plan: 1. On “Build the rows with pandas from the current EUR prices and the rates.”: No pandas in this service: plain SQL. Revise the plan.")),
                Entry::Tool(tool("Edit", "write", "~/.claude/plans/multi-currency-prices.md", "ok")),
                plan("p2", &v2, 2, "waiting", None),
            ], ..Turn::default() }],
            total: 1,
            partial: false,
            ..Conversation::default()
        };
    }
    if id != "demo-a1" {
        return Conversation {
            turns: vec![Turn { n: 1, prompt: "Read the ticket and propose a plan".into(), at: iso(60), last: iso(55), entries: vec![
                Entry::Tool(tool("Read", "read", "README.md", "ok")),
                text("Here is the plan:\n\n1. **Backend**: the endpoint and its tests\n2. **Front**: the button, behind the flag"),
                Entry::Tool(Tool { ask: Some(ask()), ..tool("AskUserQuestion", "other", "Start", "running") }),
            ], ..Turn::default() }],
            total: 1,
            partial: false,
            ..Conversation::default()
        };
    }
    Conversation {
        turns: vec![
            Turn { n: 1, prompt: "Why are credit notes rounded differently from invoices?".into(), at: iso(52), last: iso(46), tokens: 48_200, cost: Some(0.31), model: Some("claude-opus-5-5".into()), entries: vec![
                thought("Two code paths, probably two roundings. I'll read both before running anything.", 4000),
                Entry::Tool(tool("Read", "read", vat, "ok")),
                Entry::Tool(tool("Grep", "read", "roundVat", "ok")),
                Entry::Tool(Tool {
                    sub: Some(Sub {
                        steps: vec![
                            SubStep { tool: "Grep".into(), what: Some("roundVat\\(".into()), state: "ok" },
                            SubStep { tool: "Read".into(), what: Some(credit.into()), state: "ok" },
                            SubStep { tool: "Read".into(), what: Some("services/orders-api/src/money.ts".into()), state: "ok" },
                        ],
                        count: 5,
                        said: Some("`roundVat` has 3 callers; only `credit-note.ts` rounds the total.".into()),
                        running: false,
                    }),
                    ms: Some(9200),
                    agent: Some("Explore".into()),
                    ..tool("Agent", "agent", "Who calls roundVat?", "ok")
                }),
                run("ok", "Tests: 48 passed, 48 total"),
                text("Invoices round the VAT **per line**, credit notes on the **total** (`credit-note.ts` calls `roundVat` once). On a 7-line credit note the gap reaches 3 cents."),
            ], ..Turn::default() },
            Turn { n: 2, prompt: "Round per line on credit notes too, with a test that shows the gap".into(), at: iso(38), last: iso(2), tokens: 131_400, cost: Some(0.87), model: Some("claude-opus-5-5".into()), entries: vec![
                thought("", 3000),
                text("I'll write the failing test first, then align `credit-note.ts` on `vat.ts`."),
                edit("services/orders-api/src/invoices/vat.test.ts", &[" describe('credit notes', () => {", "+  it('rounds each line like an invoice', () => {", "+    expect(creditNote(lines).vat).toBe(invoice(lines).vat)", "+  })"]),
                run("failed", "✕ rounds each line like an invoice (12 ms)\n  Expected: 41.37\n  Received: 41.40\nTests: 1 failed, 48 passed, 49 total"),
                text("As expected: 3 cents apart. Now the fix."),
                edit(credit, &[" export function creditNote(lines: Line[]) {", "-  const vat = roundVat(sum(lines.map(vatOf)))", "+  const vat = sum(lines.map((l) => roundVat(vatOf(l))))"]),
                run("failed", "✕ keeps the total of a refund at zero (4 ms)\n  Expected: 0\n  Received: -0.01\nTests: 1 failed, 48 passed, 49 total"),
                Entry::Note { text: "Interrupted".into() },
                text("A full refund now leaves a cent. I'm checking how `money.ts` handles negative amounts."),
                Entry::Tool(Tool {
                    sub: Some(Sub {
                        steps: vec![
                            SubStep { tool: "Read".into(), what: Some("services/orders-api/src/money.ts".into()), state: "ok" },
                            SubStep { tool: "Grep".into(), what: Some("Math.round\\(".into()), state: "running" },
                        ],
                        count: 4,
                        said: None,
                        running: true,
                    }),
                    at: iso(20),
                    ms: Some(400),
                    agent: Some("Explore".into()),
                    ..tool("Agent", "agent", "Other negative roundings", "ok")
                }),
                thought("`roundVat` rounds half away from zero, so -0.005 becomes -0.01. Rounding the absolute value and putting the sign back keeps a refund at zero.", 7000),
                Entry::Tool(Tool { at: Some((now() - Duration::seconds(8)).to_rfc3339()), ms: None, ..tool("Bash", "command", "npm run test:invoices -- --verbose", "running") }),
            ], ..Turn::default() },
        ],
        total: 2,
        partial: false,
        plan: [("Write the failing test", "completed"), ("Round per line in credit-note.ts", "completed"), ("Keep a full refund at zero", "in_progress"), ("Run the invoice tests", "pending")]
            .iter()
            .map(|(text, status)| PlanStep { text: (*text).into(), status: (*status).into(), ..PlanStep::default() })
            .collect(),
        background: vec![
            Background { kind: "agent", what: Some("Check the other callers of roundVat".into()), state: "completed", at: iso(36), ended: iso(33), summary: Some("Agent \"Check the other callers of roundVat\" completed".into()), ..Background::default() },
            Background { kind: "command", what: Some("Start the orders API".into()), state: "running", at: iso(30), ..Background::default() },
            Background { kind: "agent", what: Some("Other negative roundings".into()), state: "running", at: iso(20), ..Background::default() },
            Background { kind: "monitor", what: Some("Watch the API log for errors".into()), state: "running", at: iso(29), events: 2, summary: Some("WARN invoice 4412: rounding gap 0.01".into()), ..Background::default() },
        ],
        live: Some(Live {
            verb: "Compiling…".into(),
            detail: vec!["2m 14s".into(), "↓ 6.2k tokens".into()],
            output: vec!["PASS  src/invoices/vat.test.ts".into(), "RUNS  src/invoices/credit-note.test.ts".into()],
        }),
        model: Some("claude-opus-5-5".into()),
        kept: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let mut s = Snapshot::default();
        Demo { now: Utc::now() }.write(&mut s);
        s
    }

    /// The demo's image: a PNG both sides, different, only for its path.
    #[test]
    fn draws_the_image_of_a_diff() {
        let s = snapshot();
        let (old, new) = (mr_image(&s, None, 1294, PLACEHOLDER, true).unwrap(), mr_image(&s, None, 1294, PLACEHOLDER, false).unwrap());
        assert!(old.starts_with("data:image/png;base64,") && old != new);
        assert!(mr_image(&s, None, 1294, "README.md", false).is_err());
    }

    /// The wizard's sample machine: signed in on the demo's own GitLab, every agent CLI listed,
    /// its repositories the snapshot's.
    #[test]
    fn detection_is_the_demos() {
        let d = detection();
        assert_eq!(d.forges[0].host, HOST.trim_start_matches("https://"));
        assert_eq!(d.forges[0].login.as_deref(), Some("alex"));
        assert_eq!(d.agents.len(), prism_core::agents::provider::Provider::ALL.len());
        assert!(d.repos.iter().any(|r| r.project.as_deref() == Some(PROJECT)));
    }

    /// Each file a demo MR changes can be read whole: the lines around its hunks.
    #[test]
    fn changed_files_are_readable() {
        let s = snapshot();
        for mr in s.merge_requests.iter().filter(|m| m.repo.is_none()) {
            let Ok(diff) = mr_diff(&s, None, mr.iid) else { continue };
            // A file shown without its diff (a lockfile folded, an image) is not read.
            for f in diff.files.iter().filter(|f| !f.new_file && !f.deleted_file && !f.diff.is_empty()) {
                let text = mr_file(&s, None, mr.iid, &f.new_path).unwrap_or_else(|e| panic!("!{} {e:#}", mr.iid));
                let file: Vec<&str> = text.lines().collect();
                // Each hunk's new side is the file's, from the line its header says.
                let mut at = 0;
                for l in f.diff.lines() {
                    if let Some(h) = l.strip_prefix("@@ ") {
                        let start = h.split(['+', ',', ' ']).find_map(|n| n.parse::<usize>().ok().filter(|_| h.contains(&format!("+{n}"))));
                        at = start.expect("a hunk's start") - 1;
                    } else if let Some(line) = l.strip_prefix(' ').or_else(|| l.strip_prefix('+')) {
                        assert_eq!(file.get(at).copied(), Some(line), "!{} {} line {}", mr.iid, f.new_path, at + 1);
                        at += 1;
                    }
                }
            }
        }
        let routes = mr_file(&s, None, 1288, "services/orders-api/src/routes.ts").unwrap();
        assert_eq!(routes.lines().nth(19), Some("  app.get('/orders', auth, orders.list)"));
    }

    #[test]
    fn hunk_counts_its_lines() {
        let h = hunk(3, 3, "fn x()", "
            | a
            -b
            +c
            +d
            ");
        assert!(h.starts_with("@@ -3,2 +3,3 @@ fn x()\n a\n-b\n+c\n+d"), "{h}");
    }

    /// Every MR opens on a description and a diff, and every thread on a line points into it.
    #[test]
    fn every_mr_has_its_review() {
        let s = snapshot();
        for m in &s.merge_requests {
            let d = mr_detail(&s, m.repo.as_deref(), m.iid).unwrap();
            let diff = mr_diff(&s, m.repo.as_deref(), m.iid).unwrap();
            assert!(!d.description.is_empty() && !diff.files.is_empty(), "!{}", m.iid);
            for t in d.discussions.iter().filter(|t| t.path.is_some()) {
                assert!(diff.files.iter().any(|f| Some(&f.new_path) == t.path.as_ref()), "!{} {}", m.iid, t.id);
            }
            assert_eq!(m.open_threads as usize, d.discussions.iter().filter(|t| t.resolvable && !t.resolved).count(), "!{}", m.iid);
        }
    }

    /// Every pipeline lists its jobs, and every job that ran has a log.
    #[test]
    fn every_pipeline_has_jobs_and_logs() {
        let s = snapshot();
        let ids = s.pipelines.iter().map(|p| p.id).chain(s.merge_requests.iter().filter_map(|m| m.head_pipeline.as_ref().map(|p| p.id)));
        for id in ids {
            for j in pipeline_jobs(&s, id).unwrap() {
                let log = job_log(&s, j.id).unwrap();
                assert!(log.log.is_empty() == matches!(j.status.name(), "manual" | "created" | "pending"), "{} {}", j.name, j.status);
            }
        }
        // The failed job a pipeline links to is the one whose log fails.
        let failed = s.merge_requests.iter().find_map(|m| m.head_pipeline.as_ref().and_then(|p| p.stages.iter().find_map(|st| st.failed_jobs.first().cloned()))).unwrap();
        let id: u64 = failed.url.rsplit('/').next().unwrap().parse().unwrap();
        assert!(job_log(&s, id).unwrap().log.contains("Job failed"));
    }

    #[test]
    fn every_ticket_opens() {
        let s = snapshot();
        for (t, _) in tickets(&s) {
            let v = ticket(&s, &t.id).unwrap();
            for sub in &v.subtasks {
                ticket(&s, &sub.task_id).unwrap();
            }
        }
        assert_eq!(ticket(&s, "SHOP-444").unwrap().subtasks.len(), 3);
    }

    #[test]
    fn every_workload_has_logs() {
        let s = snapshot();
        for e in &s.environments.as_ref().unwrap().envs {
            for w in &e.workloads {
                for p in pods(&s, &e.context, &e.namespace, &w.name).unwrap() {
                    assert!(!pod_log(&s, &e.context, &e.namespace, &p.name, false).unwrap().log.is_empty());
                }
            }
        }
    }

    #[test]
    fn every_event_has_details() {
        let s = snapshot();
        let mut events = s.agenda.as_ref().unwrap().events.clone();
        events.extend((1..7).flat_map(|d| day(now().date_naive() + Duration::days(d))));
        for e in events {
            event(&e.id).unwrap();
        }
    }

    #[test]
    fn every_agent_has_a_folder() {
        let s = snapshot();
        for a in &s.agents {
            let files = agent_files(&s, &a.id).unwrap();
            for f in files.files.iter().filter(|f| f.status.is_some()) {
                agent_preview(&s, &a.id, &f.path).unwrap();
            }
            assert!(!agent_diff(&s, &a.id, true).unwrap().files.is_empty(), "{}", a.id);
            assert!(!terminal(&s, &a.id).unwrap().is_empty());
        }
    }
}
