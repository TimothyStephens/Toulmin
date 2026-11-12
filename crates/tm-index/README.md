# tm-index

The index transaction (design 4.1, 11.17, item 26b): `IndexTx` over one
`rusqlite::Transaction`, with `execute(sql, params)`, `query_rows(sql,
params) -> Vec<Row>`, and `head(chain) -> Option<Head>`, plus the `Param`
and `Row` types folds pass and receive. Every library's fold
(`fn fold(&mut IndexTx, &Event)`, rule 6) and the Core write through it,
so no library depends on the Core, and the `rusqlite` dependency lives
only here.

`Index` opens the database (3.4: one SQLite file per machine and project)
and begins transactions; `integrity_ok` is the `PRAGMA integrity_check`
the Core runs on open. The schema (`schema.sql`, 11.17) is the Core's
(item 76): this crate knows only the `head` table's columns.
