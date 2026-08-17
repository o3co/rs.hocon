# hocon-parser — Rust 向け HOCON パーサー

[![Crates.io](https://img.shields.io/crates/v/hocon-parser.svg)](https://crates.io/crates/hocon-parser)
[![docs.rs](https://docs.rs/hocon-parser/badge.svg)](https://docs.rs/hocon-parser)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/o3co/rs.hocon/branch/develop/graph/badge.svg)](https://codecov.io/gh/o3co/rs.hocon)
[![License](https://img.shields.io/crates/l/hocon-parser.svg)](LICENSE)

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md) の Rust パーサー。手書きレキサー、再帰下降パーサー、型付き `Config` API を備え、オプションで Serde 統合に対応。現在の準拠率は [仕様準拠](#仕様準拠) を参照。

> **[Claude Code](https://claude.ai/claude-code)（Anthropic）による設計・実装。**
> [GitHub Copilot](https://github.com/features/copilot) および [OpenAI Codex](https://openai.com/index/openai-codex/) によるレビュー。

[English](README.md)

---

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

// テキスト → T の 1 ステップ
let app: App = hocon::from_str(r#"
    server {
        host = "localhost"
        port = 8080
    }
"#)?;

// ファイルからも同様
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

## 特徴

- 完全な HOCON 構文: オブジェクト、配列、コメント、複数行文字列、クォートなし文字列
- 変数参照（`${foo}`、`${?foo}`）+ 循環検出
- `include "file.conf"` / `include file("file.conf")` ディレクティブ + 相対パス解決
- 仕様準拠のオブジェクトマージ・配列連結
- 文字列・配列・オブジェクトの値連結
- Duration・バイトサイズのパース（`10 seconds`、`512 MB`）
- 環境変数の参照（`${HOME}`）
- ドット区切りパス式（`server.host`）
- フォールバック設定のマージ（`with_fallback`）
- オプションの Serde デシリアライゼーション: 1 ステップの `hocon::from_str::<T>()` /
  `from_file::<T>()`、パス指定の `Config::get_as::<T>(path)`、`Config::deserialize::<T>()`
- Lightbend 等価テスト合格（equiv01 - equiv05）

## API リファレンス

### パース

```rust
// HOCON 文字列をパース
let config = hocon::parse(input)?;

// HOCON ファイルをパース（include ディレクティブをファイル位置からの相対パスで解決）
let config = hocon::parse_file("application.conf")?;

// カスタム環境変数でパース
use std::collections::HashMap;
let env: HashMap<String, String> = HashMap::new();
let config = hocon::parse_with_env(input, &env)?;
let config = hocon::parse_file_with_env("application.conf", &env)?;
```

### 型付きゲッター

すべての型付きゲッターは `Result<T, ConfigError>` を返します。パスはドット記法を使用。

```rust
let host: String    = config.get_string("server.host")?;
let port: i64       = config.get_i64("server.port")?;
let rate: f64       = config.get_f64("rate")?;
let debug: bool     = config.get_bool("debug")?;        // "yes"/"no"、"on"/"off" も可
let sub: Config     = config.get_config("database")?;    // サブオブジェクトを Config として取得
let items: Vec<HoconValue> = config.get_list("items")?;
```

### Option バリアント

`Result` の代わりに `Option<T>` を返す。キーが存在しないか型が一致しない場合は `None`。

```rust
let host: Option<String> = config.get_string_option("server.host");
let port: Option<i64>    = config.get_i64_option("server.port");
let rate: Option<f64>    = config.get_f64_option("rate");
let debug: Option<bool>  = config.get_bool_option("debug");
```

### Duration・バイトサイズ

```rust
use std::time::Duration;

// 対応: ns, us, ms, s/seconds, m/minutes, h/hours, d/days
let timeout: Duration = config.get_duration("server.timeout")?;

// 対応: B, KB, KiB, MB, MiB, GB, GiB, TB, TiB（長い形式も可）
let max_size: i64 = config.get_bytes("upload.max-size")?;
```

### 検査

```rust
let exists: bool     = config.has("server.host");
let keys: Vec<&str>  = config.keys();           // トップレベルキー（挿入順）
let raw: Option<&HoconValue> = config.get("server.host");
```

### フォールバックマージ

```rust
// レシーバが優先。フォールバックが不足キーを補完。オブジェクトはディープマージ。
let merged = app_config.with_fallback(&defaults);
```

### Serde デシリアライゼーション

`serde` フィーチャーが必要。

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

// テキスト (またはファイル) → T の 1 ステップ — serde_json::from_str と同型:
let server: ServerConfig = hocon::from_str("host = localhost, port = 8080")?;
let server: ServerConfig = hocon::from_file("server.conf")?;

// パス指定: パス上の任意ノード (オブジェクト・配列・スカラー) をデシリアライズ:
let config = hocon::parse(input)?;
let server: ServerConfig = config.get_as("server")?;
let ports: Vec<u16> = config.get_as("ports")?;

// Config 全体からも (with_fallback / resolve の後などに):
let server: ServerConfig = config.deserialize()?;
```

## エラー型

| 型 | 発生条件 |
|------|------|
| `ParseError` | レキシング/パース時の構文エラー（行・列番号を含む） |
| `ResolveError` | 変数参照の失敗、循環参照、必須変数の欠落 |
| `ConfigError` | 値アクセス時のキー欠落・型不一致 |
| `ConfigError`（未解決の置換プレースホルダーを含むパスへのゲッター呼び出し時は `.is_not_resolved()` で検出、v1.4.0） | 未解決の置換プレースホルダーを含むパスへのゲッター呼び出し |
| `DeserializeError` | Serde デシリアライゼーション失敗（`serde` フィーチャー使用時） |

## HOCON の例

```hocon
# コメントは // または #
server {
    host = "0.0.0.0"
    port = 8080
    timeout = 30 seconds
    max-upload = 512 MB
}

# 変数参照
app {
    name = "my-app"
    title = "Welcome to "${app.name}
}

# 配列連結
base-tags = ["production"]
tags = ${base-tags} ["v2"]

# 他のファイルをインクルード
include "defaults.conf"

# クォートなし文字列
path = /usr/local/bin

# 複数行文字列
description = """
    This is a multi-line
    string value.
"""

# オブジェクトマージ
defaults { color = "blue", size = 10 }
defaults { size = 20 }  # マージ: color は保持、size は更新
```

## 仕様準拠

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md) への準拠状況は [`docs/spec-compliance.md`](docs/spec-compliance.md) に項目単位で記載しています。以下の表は 2026-05-13 時点のスナップショットです — 最新値は [`xx.hocon/docs/compliance-matrix.md`](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) を参照してください。

| 指標                                  | 状況         |
| ------------------------------------- | ------------ |
| 仕様全体（out-of-scope を含む）       | **75.6%**    |
| In-scope のみ                         | **84.0%**    |
| Lightbend `equiv01`–`equiv05` テスト  | 5/5 合格     |

## Minimum Supported Rust Version

MSRV は **1.82** です。5 つの adapter を含むすべての feature の組み合わせで同じです。
CI は 1.82 で `--all-features` のテストスイート全体を実行するため、これは主張ではなく
検証済みです。

## 関連プロジェクト

| プロジェクト | 言語 | レジストリ | 説明 |
|---------|----------|----------|-------------|
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | [npm](https://www.npmjs.com/package/@o3co/ts.hocon) | TypeScript/Node.js 向け HOCON パーサー |
| [go.hocon](https://github.com/o3co/go.hocon) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/go.hocon) | Go 向け HOCON パーサー |
| [hocon2](https://github.com/o3co/hocon2) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/hocon2) | HOCON → JSON/YAML/TOML/Properties 変換 CLI |

3 つのパーサー実装（[ts.hocon](https://github.com/o3co/ts.hocon)、[rs.hocon](https://github.com/o3co/rs.hocon)、[go.hocon](https://github.com/o3co/go.hocon)）はすべて同じ Lightbend HOCON 仕様で追跡されています — 実装ごとの準拠率は [横断ロールアップ](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) を参照してください。

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
├── application.conf    # 共有デフォルト
├── dev.conf            # include "application.conf" + 開発用オーバーライド
└── prod.conf           # include "application.conf" + 本番用オーバーライド
```

### バリデーション

- 設定のバリデーションは常にアプリケーション起動時に行い、使用時ではなく早期に検出しましょう
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

// `serde` フィーチャーが必要
let cfg: AppConfig = config.deserialize()?; // 起動時に即座に失敗
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
| `adapters-jsonc` | コメントと末尾カンマを許容する JSON | `serde_json` |
| `adapters-toml` | TOML ドキュメント | `toml` |
| `adapters-yaml` | YAML ドキュメント | `yaml-rust2` |

`adapters` は 5 つすべてを有効にします。いずれもオプトインなので、デフォルトビルドの
依存は `indexmap` だけのままです。プレーンな JSON にアダプターは不要です — HOCON は
JSON のスーパーセットなので、`hocon::parse` がそのまま受け付けます。

```sh
cargo add hocon-parser --features adapters        # 5 つすべて
cargo add hocon-parser --features adapters-env    # 必要なものだけでも可
```

外部データはデータのままです: マウントされた値の中の `${a.b}` はリテラルなテキストで
あり、参照にはなりません。そのファイルは HOCON の構文に同意していないプログラムのもの
だからです。

### 環境変数名からパスへの変換

**階層を作るのは `__` だけです。** 単一の `_` はセグメントの一部として残り、変数名の中の
リテラルな `.` はセパレーターではなくキーの *文字* として扱われます:

```text
APP_DB__MAX_CONN=10   ->  db.max_conn      （ネスト: "db" が "max_conn" を含む）
APP_FOO.BAR=flat      ->  "foo.bar"        （ドットを含む 1 つのトップレベルキー）
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
{"a": 1/*x*/2}   // 構文エラー — 数値 12 にはなりません
```

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
- **`.properties` include** — 基本的な `key=value` 形式のみ対応。複数行値（バックスラッシュ継続）、Unicode エスケープ、キーエスケープには対応していません。

API の詳細ドキュメントは [docs.rs](https://docs.rs/hocon-parser)（クレート公開後に利用可能）を参照してください。

## セキュリティに関する注意

信頼できない HOCON 入力を解析する場合、以下に注意してください：

- **include のパストラバーサル:** `include "../../../etc/passwd"` は `base_dir` からの相対パスで解決されます。信頼できない入力を解析する場合は、include パスを検証してください。
- **入力サイズ:** パーサーには入力サイズの制限がありません。信頼できない入力の場合は、`parse()` を呼ぶ前にサイズを検証してください。

## ライセンス

Apache License 2.0 — [LICENSE](LICENSE) を参照。

## 帰属

[Claude Code](https://claude.ai/claude-code) により設計・実装。
[GitHub Copilot](https://github.com/features/copilot) および [OpenAI Codex](https://openai.com/index/openai-codex/) によるレビュー。
