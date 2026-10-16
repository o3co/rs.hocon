# hocon-parser — Rust 向け HOCON パーサー

[![Crates.io](https://img.shields.io/crates/v/hocon-parser.svg)](https://crates.io/crates/hocon-parser)
[![docs.rs](https://docs.rs/hocon-parser/badge.svg)](https://docs.rs/hocon-parser)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/o3co/rs.hocon/branch/develop/graph/badge.svg)](https://codecov.io/gh/o3co/rs.hocon)
[![License](https://img.shields.io/crates/l/hocon-parser.svg)](LICENSE)

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md) の Rust パーサーです。手書きレキサー、再帰下降パーサー、型付き `Config` API を備え、オプションで Serde 統合に対応しています。現在の準拠率は [仕様準拠](#仕様準拠) を参照してください。

**ドキュメント:** [o3co.github.io/hocon](https://o3co.github.io/hocon/) — 4 実装・hocon2 CLI・共通の準拠コーパスをまとめた横断ハブです。

[English](README.md)

**ライブラリの位置づけ** — このライブラリは HOCON の設定ローダーです。目的は `.hocon` 設定ファイルを読み込み、`Config` API（`get_string`、`get_i64`、`get_f64`、`get_bool`、`get_duration`、`get_bytes`、`get_period`、`serde` フィーチャー有効時は `get_as`）で型付きアクセスを提供することです。低レベルのパーサー API ではありません。`ScalarValue` などの内部型はマイナーバージョン間で変更される可能性があります。

**言語横断の準拠テスト** — この実装は、[o3co/xx.hocon](https://github.com/o3co/xx.hocon) の共有 expected-JSON フィクスチャで [ts.hocon](https://github.com/o3co/ts.hocon)、[go.hocon](https://github.com/o3co/go.hocon)、[py.hocon](https://github.com/o3co/py.hocon) と並べてテストされており、4 つの実装すべてが同じ Lightbend HOCON 仕様を満たすことを保証しています。

## クイックスタート

### 1. インストール

```sh
cargo add hocon-parser
```

Serde サポートを有効にする場合:

```sh
cargo add hocon-parser --features serde
```

### 2. 使い方

```rust
use hocon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = hocon::parse(r#"
        server {
            host = "localhost"
            port = 8080
        }
        database {
            url = "jdbc:postgresql://localhost/mydb"
            pool-size = 10
        }
    "#)?;

    let host = config.get_string("server.host")?;
    let port = config.get_i64("server.port")?;

    println!("Server: {}:{}", host, port);
    Ok(())
}
```

自分の型へ直接デシリアライズすることもできます（`serde` フィーチャー）:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct App {
    server: Server,
}

#[derive(Deserialize)]
struct Server {
    host: String,
    port: u16,
}

// one step: text → T
let app: App = hocon::from_str(r#"
    server {
        host = "localhost"
        port = 8080
    }
"#)?;

// same, from a file
let app: App = hocon::from_file("app.conf")?;
```

## なぜ HOCON？

| | `.env` | JSON | YAML | HOCON |
|---|---|---|---|---|
| Comments | No | No | Yes | Yes |
| Nesting | No | Yes | Yes | Yes |
| References / Substitution | No | No | No | Yes (`${var}`) |
| File inclusion | No | No | No | Yes (`include`) |
| Object merging | No | No | Anchors (fragile) | Yes (deep merge) |
| Optional values | No | No | No | Yes (`${?var}`) |
| Trailing commas | N/A | No | N/A | Yes |
| Unquoted strings | Yes | No | Yes | Yes |

HOCON は単なるシリアライズ形式ではなく、**プログラムに注入するための設定言語** です。JSON / YAML / TOML はデータ構造の表現に徹しており、ファイルの重ね合わせ・環境変数・参照解決はアプリ側（Pydantic、Serde、Zod 等）の責務になります。HOCON はそれらを仕様そのものに内包しているため、プログラムが設定を受け取る時点で、フォールバックは合成済み・`${VAR}` 参照は解決済みの「1 枚の設定」になっています。「このレイヤーに値があるか？」に由来する条件分岐は、コードではなくフォーマット境界で消えます。

加えて HOCON は YAML の可読性と JSON の構造性を兼ね備えるため、フラットなキーバリュー設定を超えるユースケースには強い選択肢になります。

## モジュールの中に閉じる設定

大きなコードベースでは、設定は漏れ出しがちです。全チームが編集する 1 つの巨大なファイル、各サービスにコピーされたデフォルト値、互いのキーを読み合うモジュール。HOCON を使うと、各モジュールは自分のコードを持つのと同じように、自分の設定を持てます。

- **各モジュールは名前空間を持ち、自分のデフォルト値を同梱します。** `auth` モジュールは、すべてのデフォルト値を埋めた `auth { … }` をコードの隣に置きます。アプリケーションはそのデフォルト値をコピーせず、`include` 1 行で取り込みます。
- **アプリケーションは違う部分だけを上書きします。** 後のレイヤーが前のレイヤーにディープマージされるので、アプリケーションが書くのはブロック全体ではなく 2 つのキーだけです。
- **各モジュールは自分のスキーマを通して、自分のサブツリーだけを読みます。** モジュールは `auth` を要求し、自分で定義した型にデシリアライズします。そのセクションの形がモジュールの契約であり、ファイルのそれ以外の部分はモジュールから見えません。
- **モジュール間の配線はアプリケーションに置きます。** モジュールは自分が読むキー（`auth.db`）を宣言するだけで、アプリケーションが与えるべきキーにはデフォルト値を置きません。アプリケーションがそれを共有の値につなぎます（`auth.db = ${shared.db}`）。モジュールが互いのキーやアプリケーションのキーを参照することはないので、依存は一方向になります。

```hocon
# auth/reference.conf: shipped inside the auth package, owned by the auth team
auth {
  session { lifetime = 30m, max-body = 64KiB }
  # db: no default; the application supplies auth.db
}
```

```hocon
# application.conf: owned by the application
include package("github.com/acme/auth", "reference.conf")

shared.db { host = db.internal, port = 5432 }
auth.db = ${shared.db}       # wiring lives in the application
auth.session.lifetime = 2h   # override what differs; the rest stays the module's default
```

auth モジュールの中では（`serde` と `include-package` フィーチャー）、モジュールが自分のデフォルト値を公開し、自分のサブツリーだけを読みます:

```rust
use serde::Deserialize;
use std::time::Duration;

// src/auth.rs; the defaults sit at src/auth/reference.conf
pub fn register(parser: hocon::Parser) -> hocon::Parser {
    parser.register_package("github.com/acme/auth", "reference.conf",
                            include_str!("auth/reference.conf"))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Session {
    #[serde(with = "hocon::serde::duration")]
    pub lifetime: Duration,      // "2h"
    #[serde(with = "hocon::serde::bytes")]
    pub max_body: u64,           // "64KiB"
}

#[derive(Deserialize)]
pub struct Db { pub host: String, pub port: u16 }

#[derive(Deserialize)]
pub struct Auth { pub session: Session, pub db: Db }

pub fn load(config: &hocon::Config) -> Result<Auth, hocon::ConfigError> {
    config.get_as::<Auth>("auth")
}
```

アプリケーションは各モジュールを登録してから、自分のファイルをパースします:
`let config = auth::register(hocon::Parser::new()).parse_file("application.conf")?;`。
この例の全体は [`examples/module_config`](examples/module_config/main.rs) として実行できます
（`cargo run --example module_config --features serde,include-package`）。

`include package(...)` は HOCON.md の一部ではありません。o3co の実装が共有する規約（[E11](https://github.com/o3co/xx.hocon/blob/main/docs/extra-spec-conventions.md#e11)）で、`include classpath(...)` に相当する JVM 以外向けの仕組みです。アプリケーションがファイルの置き場所を知らなくても、モジュールが自分のデフォルト値を公開できるのはこの仕組みによります（[パッケージ include](#パッケージ-include-include-package) を参照）。JVM では Lightbend の `reference.conf` 規約が同じ役割を果たします。

## 特徴

- 完全な HOCON 構文: オブジェクト、配列、コメント、複数行文字列、クォートなし文字列
- 変数参照（`${foo}`、`${?foo}`）+ 循環検出
- `include "file.conf"` / `include file("file.conf")` ディレクティブ + 相対パス解決
- 仕様準拠のオブジェクトマージ・配列連結
- 文字列・配列・オブジェクトの値連結
- duration・バイトサイズ・period のパース（`10 seconds`、`512 MB`、`2w`）。10 進数の
  厳密な演算で計算します（`1.001s` は正確に 1001 ms）
- 環境変数の参照（`${HOME}`）
- ドット区切りパス式（`server.host`）
- フォールバック設定のマージ（`with_fallback`）
- 遅延解決のライフサイクル: `parse_string_with_options` → `with_fallback` → `resolve()`。
  Lightbend の `parseString` / `withFallback` / `resolve()` API に準拠（E12、v1.4.0）
- HOCON エミッター: `Config::render_hocon()` が解決済み設定を HOCON テキストに
  逆変換（パースし直すと同じ値ツリーに戻るラウンドトリップ保証、E18）
- オプションの Serde デシリアライゼーション: 1 ステップの `hocon::from_str::<T>()` /
  `from_file::<T>()`、パス指定の `Config::get_as::<T>(path)`、`Config::deserialize::<T>()`。
  duration・バイトサイズ・period 用の `#[serde(with = ...)]` ヘルパー付き
- 他のクレートに同梱された設定を読む `include package(...)`
  （`include-package` フィーチャー。[パッケージ include](#パッケージ-include-include-package) を参照）
- フォーマットアダプター: `.properties`、環境変数、JSON5、JSONC、TOML、YAML を
  HOCON として読めます（[フォーマットアダプター](#フォーマットアダプター) を参照）
- Lightbend 等価テスト合格（equiv01 - equiv05）

## API リファレンス

### パース

```rust
// Parse a HOCON string
let config = hocon::parse(input)?;

// Parse a HOCON file (resolves include directives relative to file location)
let config = hocon::parse_file("application.conf")?;

// Parse with custom environment variables
use std::collections::HashMap;
let env: HashMap<String, String> = HashMap::new();
let config = hocon::parse_with_env(input, &env)?;
let config = hocon::parse_file_with_env("application.conf", &env)?;
```

### 型付きゲッター

すべての型付きゲッターは `Result<T, ConfigError>` を返します。パスはドット記法を使用します。

```rust
let host: String    = config.get_string("server.host")?;
let port: i64       = config.get_i64("server.port")?;
let rate: f64       = config.get_f64("rate")?;
let debug: bool     = config.get_bool("debug")?;        // also accepts "yes"/"no", "on"/"off"
let sub: Config     = config.get_config("database")?;    // sub-object as Config
let items: Vec<HoconValue> = config.get_list("items")?;
```

### Option バリアント

`Result` の代わりに `Option<T>` を返します。キーが存在しないか型が一致しない場合は `None` を返します。

```rust
let host: Option<String> = config.get_string_option("server.host");
let port: Option<i64>    = config.get_i64_option("server.port");
let rate: Option<f64>    = config.get_f64_option("rate");
let debug: Option<bool>  = config.get_bool_option("debug");
```

### Duration・バイトサイズ

```rust
use std::time::Duration;

// Supports: ns, us, ms, s/seconds, m/minutes, h/hours, d/days (and long forms)
let timeout: Duration = config.get_duration("server.timeout")?;

// Supports: B; kB, MB, GB, TB, PB, EB, ZB, YB (powers of 10);
// Ki/KiB … Yi/YiB and the single letters K, M, G, T, P, E, Z, Y (powers of 2);
// and long forms (kilobytes, mebibytes, …)
let max_size: i64 = config.get_bytes("upload.max-size")?;

// Supports: d/days, w/weeks, m/mo/months, y/years (integer amounts)
let retention: hocon::Period = config.get_period("retention")?;
```

単位名は大文字と小文字を区別します。`KB` や `100 MS` はエラーです（キロの 10 進表記は
`kB`）。数値だけの値はデフォルトの単位（ミリ秒、バイト、日）で読みます。period は整数のみです。
duration とバイトサイズの小数の値は厳密に
計算してからゼロ方向に切り捨てます（`1.001s` は 1001 ms、`1.0009kB` は 1000 バイト）。
`std::time::Duration` で表せる範囲を超える duration はエラーです。数値には Lightbend と同じく
指数表記も使えます。`1e+5ms` は 100 秒、`2E2KiB` は 204800 バイトです（単位は末尾の英字の並びなので、
`1E` は 1 エクサバイトです）。

### 検査

```rust
let exists: bool     = config.has("server.host");
let keys: Vec<&str>  = config.keys();           // top-level keys in insertion order
let raw: Option<&HoconValue> = config.get("server.host");
```

### HOCON レンダリング

解決済み・データのみの `Config` を HOCON テキストに逆変換します（クロス実装
規約 E18。go.hocon `RenderHOCON` / ts.hocon `renderHocon` / py.hocon
`render_hocon` とロックステップ）。保証はラウンドトリップ — 出力をパースし
直すと同じ値ツリーに戻ること — であり、バイト単位のフォーマット固定では
ありません。別の型に読めてしまう文字列はクォートされたまま（`"8080"` は
文字列として再パース）、未解決の変数参照プレースホルダはエラーになります:

```rust
let text: String = config.render_hocon()?;
```

### フォールバックマージ

```rust
// Receiver wins; fallback fills missing keys. Objects are deep-merged.
let merged = app_config.with_fallback(&defaults);
```

### パッケージ include (`include package`)

`include-package` フィーチャーが必要です。設定を同梱するクレートは、その設定を
[`hocon::Parser`](https://docs.rs/hocon-parser/latest/hocon/struct.Parser.html) に登録します。
そのパーサーでパースされるドキュメントは、ファイルの置き場所を知らなくても、パッケージ識別子と
ファイル名で設定を include できます（クロス実装規約
[E11](https://github.com/o3co/xx.hocon/blob/main/docs/extra-spec-conventions.md#e11)）:

```rust
let config = hocon::Parser::new()
    .register_package("github.com/acme/auth", "reference.conf",
                      include_str!("../conf/reference.conf"))
    .parse_file("application.conf")?;
```

```hocon
include package("github.com/acme/auth", "reference.conf")
include required(package("github.com/acme/auth", "reference.conf"))  # same; required
```

- 識別子は Go のモジュールパス形式（`host/org/name`）に従うことを推奨します。これにより、1 つの
  ドキュメントが o3co のすべての実装で動きます（パーサーは空でない任意の文字列を受け付けます）。
- 他のパッケージに依存するパッケージは、`register(parser: hocon::Parser) -> hocon::Parser`
  関数を公開します。この関数は自分のファイルを登録してから、依存先の `register` を呼びます
  （カスケード規約）。
- 同じ `(identifier, file)` に異なる内容を登録すると panic します。バイト単位で同一の内容を
  再登録しても何も起きません。登録されていないパッケージを include すると、パースは
  `HoconError::Resolve` で失敗します。

### 遅延解決 (v1.4.0)

解決せずにパースし、実行時のフォールバックを追加してから、1 回のパスで解決します:

```rust
use hocon::{parse_string_with_options, ParseOptions, ResolveOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_string_with_options(
        r#"version = ${shortversion}-${CI_RUN_NUMBER}
           variables { shortversion = "1.2.3" }"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )?;
    assert!(!cfg.is_resolved());

    let runtime = hocon::empty(None); // or from_map with runtime values
    let vars = cfg.get_config("variables")?;
    let resolved = cfg
        .with_fallback(&runtime)
        .with_fallback(&vars)
        .resolve(ResolveOptions::defaults())?;

    println!("{}", resolved.get_string("version")?); // e.g. "1.2.3-42"
    Ok(())
}
```

`resolve_with` を使うと、解決済みのソースを変数参照の参照先として与えつつ、そのキーを
結果にマージしないようにできます:

```rust
let resolved = cfg.resolve_with(&source_config, ResolveOptions::defaults())?;
```

### Serde デシリアライゼーション

`serde` フィーチャーが必要です。

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

// One step, text (or file) → T — like serde_json::from_str:
let server: ServerConfig = hocon::from_str("host = localhost, port = 8080")?;
let server: ServerConfig = hocon::from_file("server.conf")?;

// Path-scoped: deserialize any node (object, array, or scalar) at a path:
let config = hocon::parse(input)?;
let server: ServerConfig = config.get_as("server")?;
let ports: Vec<u16> = config.get_as("ports")?;

// Or the whole Config, e.g. after with_fallback / resolve:
let server: ServerConfig = config.deserialize()?;
```

文字列は、フィールドが求める数値・真偽値・char として、getter と同じ規則で読みます。環境変数は常に文字列ですが、
`PORT=8080` のとき `port = ${?PORT}` は `u16` に `8080` として入ります。struct・`Vec`・map の値・タプル・`Option`・
newtype の中でも同じです。map のキーの型が数値・真偽値・char・unit variant の enum なら、キーを `str::parse` で読みます
（真偽値のキーは `true` か `false`）。読めないキーは文字列のままキーの型に渡します。ただし、serde がフィールドの型を知る前に値をバッファする場所
（`#[serde(flatten)]`、`#[serde(untagged)]`、internally tagged（`#[serde(tag = "...")]`）の enum）では、値は書かれた
ままの形でフィールドに届くので、文字列は文字列のままです。そこにある数値や真偽値のフィールドは `"8080"` や `"yes"` を
受け付けず、untagged の `N(i64), S(String)` は `"8080"` を `S` として読みます。そうした値は引用符なしの数値・`true`・
`false` で書くか、adjacently tagged の enum（`#[serde(tag = "t", content = "c")]`）をタグがコンテンツより先に来る形で
使うか、`#[serde(deserialize_with = ...)]` でフィールド側で変換してください。

単位付きの値（duration・バイトサイズ・period）は、単位ごとの `with` モジュールを通して
`get_duration` / `get_bytes` / `get_period` と同じ規則で読めます。範囲の制約はフィールドの型に任せます:

```rust
use std::time::Duration;

#[derive(Deserialize)]
struct Session {
    #[serde(with = "hocon::serde::duration")]
    lifetime: Duration,          // lifetime = 30m
    #[serde(with = "hocon::serde::bytes")]
    max_body: u64,               // max_body = 64KiB (any T: TryFrom<i64>)
    #[serde(with = "hocon::serde::period")]
    retention: hocon::Period,    // retention = 2w
    #[serde(default, with = "hocon::serde::duration::option")]
    idle_timeout: Option<Duration>,  // null or missing → None
}
```

単位のない数値はゲッターと同じくデフォルトの単位で読みます（`lifetime = 30` は 30 ms）。単位の明記をフィールドの
契約にするには、ヘルパーの `require_unit` 版（`duration::require_unit`・`bytes::require_unit`・
`period::require_unit`。それぞれ `::option` あり）を使います。数値や数値だけの文字列（環境変数から来る `"30"`）は
そのキーで `duration must name its unit`（`byte size …`・`period …`。メッセージに値は含めません）として拒否され、
`30s`・`"30 s"`・`64KiB`・`2w` はこれまでどおり読めます。そもそも読めない値は従来のエラーのままです。

```rust
#[derive(Deserialize)]
struct Token {
    #[serde(with = "hocon::serde::duration::require_unit")]
    ttl: Duration,               // ttl = 30s → 30 s; ttl = 30 → error at `ttl`
}
```

デコードエラーには失敗したキーのパスが入ります: `DeserializeError::path`（および `get_as` の
`ConfigError::path`、`from_str` / `from_file` では `HoconError::Config` の `ConfigError::path`）が
`session.lifetime`、`servers[0].port` などになります。ヘルパーは HOCON の値をゲッターとまったく同じに読みます。
serde が値をバッファする場所（`#[serde(flatten)]`、untagged / internally tagged enum）でも同じ値を読みます。
例外は、i64 の整数ではなく有効数字が 15 桁を超える数値リテラル（最も近い double としてバッファされます）と、
double でアンダーフローする負の数（`-1e-400`。`-0.0` としてバッファされます）です。また、エラーのパスは
バッファされたオブジェクトで止まります。ほかの自己記述的な Deserializer（serde_json など）では数値をその値から
読み、整数は整数として、浮動小数点数は浮動小数点数として読みます。自己記述的でないフォーマット
（bincode、postcard）ではヘルパーを使えません。

## エラー型

| 型 | 発生条件 |
|------|------|
| `HoconError` | パース関数、`from_str` / `from_file`、`Config::resolve` が返します。以下のいずれかのエラー（`Parse`、`Resolve`、`Io`、`NotResolved`、`Config`）を包みます |
| `ParseError` | レキシング/パース時の構文エラー（行・列番号を含む） |
| `ResolveError` | 変数参照の失敗、循環参照、必須変数の欠落 |
| `NotResolvedError` | 未解決の変数参照が残っているソース設定に対して `resolve_with` を呼んだ |
| `ConfigError` | 値アクセス時のキー欠落・型不一致 |
| `ConfigError`（「値がまだ解決されていない」は `.is_not_resolved()` で検出） | 未解決の置換プレースホルダーを含むパスへのゲッター呼び出し（v1.4.0） |
| `DeserializeError` | Serde デシリアライゼーション失敗。失敗したキーの `path` を含む（`serde` フィーチャー使用時） |
| `AdapterError` | フォーマットアダプターが入力を読めなかった（`adapters-*` フィーチャー使用時） |

## HOCON の例

```hocon
# Comments start with // or #
server {
    host = "0.0.0.0"
    port = 8080
    timeout = 30 seconds
    max-upload = 512 MB
}

# Substitutions
app {
    name = "my-app"
    title = "Welcome to "${app.name}
}

# Array concatenation
base-tags = ["production"]
tags = ${base-tags} ["v2"]

# Include other files
include "defaults.conf"

# Unquoted strings
path = /usr/local/bin

# Multi-line strings
description = """
    This is a multi-line
    string value.
"""

# Object merging
defaults { color = "blue", size = 10 }
defaults { size = 20 }  # merges: color stays, size updated
```

## パフォーマンス

[Criterion](https://github.com/bheisler/criterion.rs) で計測しています。各イテレーションにはパースと 1 回の `get_string` 参照が含まれます。再現するには `cargo bench` を実行してください。

| シナリオ | ops/sec | 1 回あたりの時間 |
|---|---|---|
| 小規模設定（10 キー） | ~62,000 | ~16 µs |
| 中規模設定（100 キー） | ~19,000 | ~52 µs |
| 大規模設定（1,000 キー） | ~2,400 | ~408 µs |
| 変数参照 10 個 | ~37,000 | ~27 µs |
| 変数参照 50 個 | ~12,000 | ~86 µs |
| 変数参照 100 個 | ~6,400 | ~156 µs |
| ネスト深度 5 | ~58,000 | ~17 µs |
| ネスト深度 10 | ~50,000 | ~20 µs |
| ネスト深度 20 | ~39,000 | ~26 µs |

一般的なアプリケーション設定（起動時に 1 回だけ読み込む）では、パースのコストは無視できます — 1,000 キーの設定でも 0.5 ms 未満でパースできます。

## 比較

✅ 完全対応 / ⚠️ 部分対応 / ❌ 非対応

### HOCON 実装

| 機能 | rs.hocon | [hocon-rs](https://github.com/mockersf/hocon.rs) |
|---|:---:|:---:|
| 変数参照（`${path}`） | ✅ | ✅ |
| オプショナルな変数参照（`${?path}`） | ✅ | ✅ |
| Include | ✅ | ✅ |
| `include required(file(...))` | ✅ | ❌ |
| オブジェクト/配列の連結 | ✅ | ✅ |
| 型変換 | ✅ | ⚠️ |
| Duration のパース | ✅ | ✅ |
| バイトサイズのパース | ✅ | ✅ |
| `+=` 追記 | ✅ | ❌ |
| Serde デシリアライゼーション | ✅ | ✅ |
| 環境変数へのフォールバック | ✅ | ❌ |
| 循環 include の検出 | ✅ | ❌ |

### 設定フレームワーク

| | rs.hocon | [config-rs](https://github.com/mehcode/config-rs) |
|---|:---:|:---:|
| **フォーマット** | | |
| HOCON | ✅ | ❌ |
| JSON | ✅ | ✅ |
| JSON5 | ✅ (adapter) | ✅ |
| JSONC | ✅ (adapter) | ⚠️ (via JSON5) |
| YAML | ✅ (adapter) | ✅ |
| TOML | ✅ (adapter) | ✅ |
| 環境変数 | ✅ (adapter, and `${VAR}` fallback) | ✅ |
| .properties | ✅ (adapter, and `include`) | ❌ |
| **機能** | | |
| 変数参照 | ✅ | ❌ |
| ファイル include | ✅ | ❌ |
| 型変換 | ✅ | ✅ |
| Serde サポート | ✅ | ✅ |
| 監視/リロード | ❌ | ❌ |
| レイヤー構成 | ✅ (`include`, `with_fallback`) | ✅ |

## 仕様準拠

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md) への準拠状況は [`docs/spec-compliance.md`](docs/spec-compliance.md) に項目単位で記録しており、以下の準拠率はこのファイルから計算しています — `tests/docs.rs` が準拠率を再計算し、README.md の表がずれるとビルドを失敗させます。実装横断のロールアップは [`xx.hocon/docs/compliance-matrix.md`](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) を参照してください。

| 指標                                  | 状況         |
| ------------------------------------- | ------------ |
| 仕様全体（out-of-scope を含む）       | **92.9%**    |
| In-scope のみ                         | **100.0%**   |
| Lightbend `equiv01`–`equiv05` テスト  | 5/5 合格     |

## Minimum Supported Rust Version

MSRV は **1.82** です。6 つの adapter を含むすべての feature の組み合わせで同じです。
CI は 1.82 で `--all-features` のテストスイート全体を実行するため、これは主張ではなく
検証済みです。

## 関連プロジェクト

| プロジェクト | 言語 | レジストリ | 説明 |
|---------|----------|----------|-------------|
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | [npm](https://www.npmjs.com/package/@o3co/ts.hocon) | TypeScript/Node.js 向け HOCON パーサー |
| [go.hocon](https://github.com/o3co/go.hocon) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/go.hocon) | Go 向け HOCON パーサー |
| [py.hocon](https://github.com/o3co/py.hocon) | Python | [PyPI](https://pypi.org/project/hocon-parser/) | Python 向け HOCON パーサー |
| [hocon2](https://github.com/o3co/hocon2) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/hocon2) | HOCON → JSON/YAML/TOML/Properties 変換 CLI |

4 つのパーサー実装（[ts.hocon](https://github.com/o3co/ts.hocon)、[rs.hocon](https://github.com/o3co/rs.hocon)、[go.hocon](https://github.com/o3co/go.hocon)、[py.hocon](https://github.com/o3co/py.hocon)）はすべて同じ Lightbend HOCON 仕様で追跡されています — 実装ごとの準拠率は [横断ロールアップ](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) を参照してください。

## ベストプラクティス

### 設定構成

- **ドメインごとに分割**: 設定を論理的な単位に分けましょう（`database.conf`、`server.conf`、`logging.conf`）
- **`include` で合成**: ドメイン別ファイルからフル設定を組み立てましょう
- **設定にロジックを入れない**: HOCON は宣言的なデータのためのもので、条件分岐や計算には向きません

### 環境変数

- **`${ENV}` の使用を最小限に**: 設定ファイル自体にデフォルト値を定義し、`${?ENV}`（オプショナル）を使いましょう
- **ローカル開発で環境変数を必須にしない**: デフォルトだけで動くようにしましょう
- **必須の環境変数を文書化**: プロジェクトの README や `.env.example` にリストしましょう

**UTF-8 でないエントリがパースを中断させることはありません。** 名前または値が妥当な
UTF-8 でない環境変数エントリは、このクレートが `${...}` を解決するすべての箇所
（`parse`、`parse_file`、`Parser::parse`、`Parser::parse_file`、および
`use_system_environment` 付きの `Config::resolve` / `Config::resolve_with`）で
「存在しない」ものとして扱われます。そのような変数を指す `${VAR}` は、その変数が
**未設定** の場合とまったく同じに振る舞います — `${?VAR}` は未定義となり、`${VAR}` は
通常の「未解決の置換」エラーになります。ロス付き変換されたテキストに解決されることは
ないため、UTF-8 でないバイト列が壊れたデータとして設定に紛れ込むことはありません。

これによって既存の動く設定の意味が変わることはありません: `std::env::vars()` は
ドキュメントがどの変数を参照しているかに関係なく、最初の不正なエントリでパニックして
いたため、影響を受けるユーザーのこれまでの挙動は「クラッシュ」であって「正常なパース」
ではなかったからです。

**bulk mount だけは例外で、エラーになります** —
[フォーマットアダプター](#フォーマットアダプター) を参照してください。

### 開発 / 本番の分離

```text
config/
├── application.conf    # shared defaults
├── dev.conf            # include "application.conf" + dev overrides
└── prod.conf           # include "application.conf" + prod overrides
```

### バリデーション

- 設定のバリデーションは使用時ではなく、常にアプリケーション起動時に行いましょう
- スキーマバリデーション（TypeScript は Zod、Go は struct Unmarshal、Rust は Serde）を使って早期にエラーをキャッチしましょう

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Deserialize)]
struct AppConfig {
    server: ServerConfig,
    debug: bool,
}

// requires the `serde` feature
let cfg: AppConfig = config.deserialize()?; // fails fast on startup
```

## フォーマットアダプター

*他の* プログラムが所有する設定ファイルを HOCON としてマウントできます。これにより、
自分のドキュメント内の `${...}` からそれらの値を参照できます:

```rust
use hocon::adapters::env;

// APP_DB__HOST=db.internal  ->  db.host
let base = env::load(env::Options { prefix: "APP_".into(), ..Default::default() })?;

let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
let cfg = hocon::parse_string_with_options(src, opts)?;
let merged = cfg.with_fallback(&base).resolve(hocon::ResolveOptions::defaults())?;
```

解決を遅延させることが重要です: 通常の `parse` は読みながら解決するため、フォール
バックを指す `${...}` はフォールバックが接続される前に失敗してしまいます。

| フィーチャー | アダプター | 追加依存 |
| --- | --- | --- |
| `adapters-properties` | `java.util.Properties`（`include` 構文レイヤーを共有） | — |
| `adapters-env` | プレフィックス付き名前空間の一括マウント。`.env` も読めます | — |
| `adapters-json5` | JSON5 ドキュメント（JSON5 1.0.0、手書きスキャナー） | — |
| `adapters-jsonc` | コメントと末尾カンマを許容する JSON | `serde_json` |
| `adapters-toml` | TOML ドキュメント | `toml` |
| `adapters-yaml` | YAML ドキュメント | `yaml-rust2` |

`adapters` は 6 つすべてを有効にします。いずれもオプトインなので、デフォルトビルドの
依存は `indexmap` だけのままです。プレーンな JSON にアダプターは不要です — HOCON は
JSON のスーパーセットなので、`hocon::parse` がそのまま受け付けます。

```sh
cargo add hocon-parser --features adapters        # all six
cargo add hocon-parser --features adapters-env    # or just the one you need
```

外部データはデータのままです: マウントされた値の中の `${a.b}` はリテラルなテキストで
あり、参照にはなりません。そのファイルは HOCON の構文に同意していないプログラムのもの
だからです。

### 環境変数名からパスへの変換

**階層を作るのは `__` だけです。** 単一の `_` はセグメントの一部として残り、変数名の中の
リテラルな `.` はセパレーターではなくキーの *文字* として扱われます:

```text
APP_DB__MAX_CONN=10   ->  db.max_conn      (nested: "db" contains "max_conn")
APP_FOO.BAR=flat      ->  "foo.bar"        (one top-level key that contains a dot)
```

後者は単一のキーなので、クォート付きパス `cfg.get_string("\"foo.bar\"")` で読みます。
一方 `APP_FOO__BAR` は `cfg.get_string("foo.bar")` で読みます。両者は別々のパスなので、
同時に設定しても衝突しません。セグメントは変換後に小文字化されます。

*実際に* 同じパスへ変換される 2 つの変数（`APP_A__B` と `APP_a__b`）は、暗黙の
last-wins ではなくエラーになります。環境変数の列挙順は決定的ではないからです。`.env`
ファイルには明確な行順があるため、そちらでは通常どおり後の行が優先されます。

`${VAR}` とは異なり、`adapters::env::load` は mount prefix に一致するエントリの名前
または値が妥当な UTF-8 でない場合に **エラー** になります。bulk mount は名前空間全体を
要求する操作なので、1 つのキーを黙って落とすと「完全に見えるのに operator の設定だけが
欠けたサブツリー」を返してしまい、古い設定のデフォルト値が何の兆候もないまま勝って
しまうからです。prefix に一致しないエントリはデコードの可否によらず無視されるため、
無関係な不正エントリが mount を失敗させることはありません。

### JSONC のコメントはトークンを分離します

コメントは削除されるのではなく空白に置き換えられるため、前後のトークンが連結されて
しまうことはありません:

```jsonc
{"a": 1/*x*/2}   // syntax error — NOT the number 12
```

### JSON5 は前処理ではなくスキャンします

JSON5 はトークンの文法そのものを変えます — クォートなしの識別子キー、シングルクォート、
16 進整数、行継続。そのため `adapters::json5` は JSON デコーダーの前段に置くプリプロセッサーでは
なく、手書きのスキャナーと再帰下降パーサーであり、追加の依存も不要です。受け付ける文法は、
リファレンス実装（npm の json5 パッケージ）が定義する JSON5 1.0.0 です。マッピング仕様が JSON5
より厳しい箇所では仕様が優先します: 整数は `i64` に収まる必要があり（F0.5）、`Infinity` / `NaN`
はエラー（F0.6）、対になっていない `\uXXXX` サロゲートはエラー（F3.5）、重複キーは HOCON の
意味論に従います — オブジェクトはマージされ、それ以外は後の値が優先されます（F0.7）。

### YAML のスカラー解決はライブラリの答えです

YAML については、スカラーの解決はこのクレートではなくライブラリの責務です:
`010` が 8 なのか 10 なのかは `yaml-rust2` の答えです。`adapters::yaml::from_value` は
デコード済みのツリーを受け取るので、別のライブラリやスキーマが必要な呼び出し側は
自分でデコードして、その結果を渡せます。

## 既知の制約

- **`include url(...)`** は未対応です。リモート設定の取得はパーサーのスコープ外です。アプリケーションの HTTP クライアントでコンテンツを取得し、`parse()` に渡してください。
- **`include classpath(...)`** は未対応です。これは JVM 固有の include 形式で、Java ランタイム外には同等の仕組みがありません。
- **監視/リロード機能なし** — 設定はロード時に解析されます。ライブリロードには、変更時に `parse()` や `parse_file()` を再度呼び出してください。
- **ストリーミングパーサーなし** — 入力全体がメモリに読み込まれます。

API の詳細ドキュメントは [docs.rs](https://docs.rs/hocon-parser) を参照してください。

## セキュリティに関する注意

信頼できない HOCON 入力を解析する場合、以下に注意してください：

- **include のパストラバーサル:** `include "../../../etc/passwd"` は `base_dir` からの相対パスで解決されます。信頼できない入力を解析する場合は、include パスを検証してください。
- **入力サイズ:** パーサーには入力サイズの制限がありません。信頼できない入力の場合は、`parse()` を呼ぶ前にサイズを検証してください。
- **ドキュメントのネスト深度:** オブジェクトと配列のネストは 128 段までに制限されており、
  `parse` と `from_map` の両方で強制されます。ランタイム自身の再帰エラーを捕捉する兄弟実装と
  異なり、Rust でスタックを使い切ると `SIGABRT` になります — 呼び出し元のプロセスごと落ち、
  `catch_unwind` でも止められないため、制限はオーバーフローの後ではなく前に置く必要があります。
  128 は `serde_json` の制限で、`jsonc` アダプターが以前から強制しているものであり、spawn した
  スレッドが得る 2 MiB のスタックでも保たれます。
- **マッピングされるパスの深さ:** 環境変数の `__` セグメントと `.properties` のドット区切りキーは
  64 セグメントまでに制限されます。ts.hocon と py.hocon と同じです。

## ライセンス

[Apache License, Version 2.0](LICENSE) の下でライセンスされています。

## 帰属

[Claude Code](https://claude.ai/claude-code) により設計・実装。
[GitHub Copilot](https://github.com/features/copilot) および [OpenAI Codex](https://openai.com/index/openai-codex/) によるレビュー。
