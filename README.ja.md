# hocon-parser — Rust 向け HOCON パーサー

[![Crates.io](https://img.shields.io/crates/v/hocon-parser.svg)](https://crates.io/crates/hocon-parser)
[![docs.rs](https://docs.rs/hocon-parser/badge.svg)](https://docs.rs/hocon-parser)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/o3co/rs.hocon/branch/main/graph/badge.svg)](https://codecov.io/gh/o3co/rs.hocon)
[![License](https://img.shields.io/crates/l/hocon-parser.svg)](LICENSE)

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md)に完全準拠した Rust パーサー。手書きレキサー、再帰下降パーサー、型付き `Config` API を備え、オプションで Serde 統合に対応。

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

HOCON は YAML の可読性と JSON の構造性を兼ね備え、どちらにもない機能 — 変数参照、インクルード、ディープマージ — を提供します。設定がフラットなキーバリューペア以上のものであれば、HOCON を検討する価値があります。

## 特徴

- 完全な HOCON 構文: オブジェクト、配列、コメント、複数行文字列、クォートなし文字列
- 変数参照（`${foo}`、`${?foo}`）+ 循環検出
- `include` ディレクティブ（file、classpath、URL）+ 相対パス解決
- 仕様準拠のオブジェクトマージ・配列連結
- 文字列・配列・オブジェクトの値連結
- Duration・バイトサイズのパース（`10 seconds`、`512 MB`）
- 環境変数の参照（`${HOME}`）
- ドット区切りパス式（`server.host`）
- フォールバック設定のマージ（`with_fallback`）
- オプションの Serde デシリアライゼーション
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

let config = hocon::parse(input)?;
let server: ServerConfig = config
    .get_config("server")?
    .deserialize()?;
```

## エラー型

| 型 | 発生条件 |
|------|------|
| `ParseError` | レキシング/パース時の構文エラー（行・列番号を含む） |
| `ResolveError` | 変数参照の失敗、循環参照、必須変数の欠落 |
| `ConfigError` | 値アクセス時のキー欠落・型不一致 |
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

[Lightbend HOCON 仕様](https://github.com/lightbend/config/blob/main/HOCON.md)への完全準拠を目標としています。テストスイートには Lightbend 等価テスト（equiv01 - equiv05）を含み、オブジェクトマージ、配列連結、変数参照、その他仕様で定義されたすべての動作を検証しています。

## Minimum Supported Rust Version

MSRV は **1.82** です。

## 関連プロジェクト

| プロジェクト | 言語 | レジストリ | 説明 |
|---------|----------|----------|-------------|
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | [npm](https://www.npmjs.com/package/@o3co/ts.hocon) | TypeScript/Node.js 向け HOCON パーサー |
| [go.hocon](https://github.com/o3co/go.hocon) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/go.hocon) | Go 向け HOCON パーサー |
| [hocon2](https://github.com/o3co/hocon2) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/hocon2) | HOCON → JSON/YAML/TOML/Properties 変換 CLI |

すべての実装が Lightbend HOCON 仕様に完全準拠しています。

## ベストプラクティス

### 設定構成

- **ドメインごとに分割**: 設定を論理的な単位に分けましょう（`database.conf`、`server.conf`、`logging.conf`）
- **`include` で合成**: ドメイン別ファイルからフル設定を組み立てましょう
- **設定にロジックを入れない**: HOCON は宣言的なデータのためのもので、条件分岐や計算には向きません

### 環境変数

- **`${ENV}` の使用を最小限に**: 設定ファイル自体にデフォルト値を定義し、`${?ENV}`（オプショナル）を使いましょう
- **ローカル開発で環境変数を必須にしない**: デフォルトだけで動くようにしましょう
- **必須の環境変数を文書化**: プロジェクトの README や `.env.example` にリストしましょう

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

## セキュリティに関する注意

信頼できない HOCON 入力を解析する場合、以下に注意してください：

- **include のパストラバーサル:** `include "../../../etc/passwd"` は `base_dir` からの相対パスで解決されます。信頼できない入力を解析する場合は、include パスを検証してください。
- **入力サイズ:** パーサーには入力サイズの制限がありません。信頼できない入力の場合は、`parse()` を呼ぶ前にサイズを検証してください。

## ライセンス

Apache License 2.0 — [LICENSE](LICENSE) を参照。

## 帰属

[Claude Code](https://claude.ai/claude-code) により設計・実装。
[GitHub Copilot](https://github.com/features/copilot) および [OpenAI Codex](https://openai.com/index/openai-codex/) によるレビュー。
