// Japanese validation adapter. Unknown diagnostics stay behind the reviewed
// generic error instead of exposing internal runtime details.
import { JAPANESE_DISCORD_MESSAGES } from "./japanese-i18n-draft.mjs";

export const JAPANESE_VALIDATION_MESSAGES = new Map([
  ["Enter a Clearra command.", "Clearraのコマンドを入力してください。"],
  ["The command has too many arguments.", "コマンドの引数が多すぎます。"],
  ["The command is too long.", "コマンドが長すぎます。"],
  ["The command contains an unterminated quote.", "コマンド内の引用符が閉じられていません。"],
  ["File and custom-code inputs are not available through Discord.", "Discordではファイルパスや独自コードを入力できません。"],
  ["target must contain at least one occupied cell.", "目標フィールドにはブロックが1個以上必要です。"],
  ["base and target must not overlap; target contains only cells to add.", "既存フィールドと目標フィールドは重ねられません。目標フィールドには追加するブロックだけを入力してください。"],
  ["target occupied-cell count must be divisible by four.", "目標フィールドのブロック数は4の倍数である必要があります。"],
  ["base must not contain an already completed row.", "既存フィールドに完成済みのラインを含めることはできません。"],
  ["next must be a queue or pattern, not a command-line option.", "ネクストにはコマンドのオプションではなく、キューまたはパターンを入力してください。"],
  ["next must be an exact queue containing only IOTSZJL pieces.", "ネクストにはIOTSZJLだけで構成した順序の確定したキューを入力してください。"],
  ["field URL is invalid.", "フィールドのURLが無効です。"],
  ["field URL must contain exactly one CTK3 or Fumen document.", "フィールドのURLにはCTK3またはFumenドキュメントを1個だけ含めてください。"],
  ["field document cannot be empty.", "フィールドのドキュメントを空にすることはできません。"],
  ["field must contain one raw CTK3 or v115 Fumen document.", "フィールドにはCTK3またはv115 Fumenドキュメントを1個、そのまま入力してください。"],
  ["v110 Fumen is not supported by the Clearra search decoder; use v115.", "Clearraの検索はv110 Fumenに対応していません。v115を使用してください。"],
  ["CTK3 search fields must be exactly 10 columns wide.", "検索用のCTK3フィールドの幅は10列である必要があります。"],
  ["CTK3 field could not be decoded.", "CTK3フィールドを読み取れませんでした。"],
  ["next pattern alternatives must have the same piece count.", "ネクストパターンの各候補はミノ数を同じにしてください。"],
  ["next pattern must contain at least one piece.", "ネクストパターンにはミノが1個以上必要です。"],
  ["next '*' must be followed immediately by ! or pN.", "ネクストの`*`の直後に、空白を入れずに`!`または`pN`を指定してください。"],
  ["next pattern contains an empty alternative.", "ネクストパターンに空の候補を含めることはできません。"],
  ["next '*' must be followed by ! or pN.", "ネクストの`*`の後に`!`または`pN`を指定してください。"],
  ["next standard-bag draws may not exceed seven pieces per group.", "ネクストの標準バッグでは、1グループから選ぶミノは7個以下にしてください。"],
  ["next pattern has an unterminated piece group.", "ネクストパターンのミノグループが閉じられていません。"],
  ["next pattern contains an invalid piece group.", "ネクストパターンのミノグループが無効です。"],
  ["next pattern piece group must leave at least one choice.", "ネクストパターンのミノグループには、選択できるミノを1種類以上残してください。"],
  ["next pattern has an unexpected bag token after a piece group.", "ネクストパターンのミノグループの後に、使用できないバッグ記号があります。"],
  ["next pattern draws more pieces than its group contains.", "ネクストパターンでは、グループに含まれる数より多くのミノを選べません。"],
  ["next pattern draw count is missing.", "ネクストパターンで選ぶミノの数を入力してください。"],
  ["next pattern draw count must be a positive integer.", "ネクストパターンで選ぶミノの数は正の整数にしてください。"],
  ["lines and legacy options clear/lines may not be specified together.", "ライン数の入力と、従来のoptions内のclear/lines設定は同時に指定できません。"],
  ["options clear must be an integer from 1 through 6.", "optionsのclearには1から6までの整数を指定してください。"],
  ["options type must be TSS, TSD, TST, TSPIN, T-SPIN, or ANY; TSM is unavailable.", "optionsのtypeはTSS、TSD、TST、TSPIN、T-SPIN、ANYのいずれかにしてください。TSMは利用できません。"],
  ["--arguments requires exactly one command name.", "/helpの--argumentsにはコマンド名を1個だけ指定してください。"],
  ["Text command /help accepts at most one command name.", "テキストコマンドの/helpに指定できるコマンド名は1個までです。"],
  ["help arguments cannot be empty.", "/helpのコマンド引数を空にすることはできません。"],
  ["help arguments exceeds the 64-character limit.", "/helpのコマンド引数は64文字以下にしてください。"],
  ["The command contains an unterminated code block.", "コマンド内のコードブロックが閉じられていません。"],
  ["A command code block cannot be empty.", "コマンドのコードブロックを空にすることはできません。"],
  ["remaining must contain only IOTSZJL pieces.", "残りのミノにはIOTSZJLだけを使用してください。"],
  ["priority must be all, build, or pc.", "セットアップの並び順にはall、build、pcのいずれかを指定してください。"],
  ["queue-knowledge must be full-queue or visible-7.", "キューの公開範囲にはfull-queueまたはvisible-7を指定してください。"],
  ["setup-length must be auto, longer, or shorter.", "セットアップの長さにはauto、longer、shorterのいずれかを指定してください。"],
  ["When next-cycle-remaining or setup-length is set, remaining must also be supplied directly.", "次のサイクルに残すミノやセットアップの長さを設定する場合は、残りのミノもスラッシュコマンドに直接入力してください。"],
]);

export const JAPANESE_INPUT_NAMES = Object.freeze({
  arguments: "コマンド", image: "画像", field: "フィールド", base: "既存フィールド",
  target: "目標フィールド", next: "ネクスト", lines: "ライン数", kicktable: "キック表",
  options: "オプション", remaining: "残りのミノ", priority: "セットアップの並び順",
  "max-setup-pieces": "セットアップの最大ミノ数", "queue-knowledge": "キューの公開範囲",
  "next-cycle-remaining": "次のサイクルに残すミノ", "setup-length": "セットアップの長さ",
  scope: "範囲",
});

const generic = () => JAPANESE_DISCORD_MESSAGES["error.validation"];
function named(name, format) {
  const label = JAPANESE_INPUT_NAMES[String(name).trim().toLowerCase()];
  return label === undefined ? generic() : format(label);
}

export const JAPANESE_VALIDATION_PATTERNS = Object.freeze([
  [/^(.+) is required in the Clearra command Modal\.$/, (name) => named(name, (label) => `${label}の入力が必要です。`)],
  [/^\/(.+) input is required\.$/, (name) => named(name, (label) => `${label}の入力が必要です。`)],
  [/^(.+) cannot be empty\.$/, (name) => named(name, (label) => `${label}を空にすることはできません。`)],
  [/^(.+) must be text\.$/, (name) => named(name, (label) => `${label}はテキストで入力してください。`)],
  [/^(.+) exceeds the (\d+)-character limit\.$/, (name, limit) => named(name, (label) => `${label}は${limit}文字以下にしてください。`)],
  [/^(.+) must be an integer from (\d+) through (\d+)\.$/, (name, min, max) => named(name, (label) => `${label}は${min}から${max}までの整数にしてください。`)],
  [/^(.+) must contain from 1 through 7 pieces\.$/, (name) => named(name, (label) => `${label}にはミノを1～7個入力してください。`)],
  [/^(.+) must contain only IOTSZJL pieces\.$/, (name) => named(name, (label) => `${label}にはIOTSZJLだけを使用してください。`)],
  [/^(.+) allows at most one piece kind twice; no piece may appear three times\.$/, (name) => named(name, (label) => `${label}では、2個使用できるミノは1種類だけです。同じミノを3個使用することはできません。`)],
  [/^next-cycle-remaining must contain exactly (\d+) pieces? when remaining contains (\d+)\.$/, (expected, remaining) => `残りのミノが${remaining}個の場合、次のサイクルに残すミノは${expected}個にしてください。`],
  [/^(.+) grid rows must be exactly 10 columns wide\.$/, (name) => named(name, (label) => `${label}の各行は横10マスで入力してください。`)],
  [/^(.+) grid must contain from one through (six|twenty-four) rows\.$/, (name, maximum) => named(name, (label) => `${label}は1～${maximum === "six" ? "6" : "24"}行で入力してください。`)],
  [/^(.+) CTK3 must contain exactly one page\.$/, (name) => named(name, (label) => `${label}のCTK3にはページを1個だけ含めてください。`)],
  [/^(.+) CTK3 exceeds the (\d+)-row limit\.$/, (name, rows) => named(name, (label) => `${label}のCTK3は${rows}行以下にしてください。`)],
  [/^(.+) Fumen could not be decoded\.$/, (name) => named(name, (label) => `${label}のFumenを読み取れませんでした。`)],
  [/^(.+) Fumen must contain exactly one page\.$/, (name) => named(name, (label) => `${label}のFumenにはページを1個だけ含めてください。`)],
  [/^(.+) requires a value\.$/, (name) => named(name, (label) => `${label}の値を入力してください。`)],
]);

function interpolate(template, values) {
  return template.replace(/\{([a-z0-9_]+)\}/gi, (placeholder, name) =>
    Object.hasOwn(values, name) ? String(values[name]) : placeholder);
}

export function japaneseValidationErrorText(error) {
  let translated = generic();
  if (error?.name === "DiscordInputError" && typeof error?.code === "string") {
    const template = JAPANESE_DISCORD_MESSAGES[`input.${error.code}`];
    if (template !== undefined) translated = interpolate(template, error.details ?? {});
  } else {
    const message = error instanceof Error ? error.message : String(error ?? "");
    translated = JAPANESE_VALIDATION_MESSAGES.get(message) ?? generic();
    if (!JAPANESE_VALIDATION_MESSAGES.has(message)) {
      for (const [pattern, replacement] of JAPANESE_VALIDATION_PATTERNS) {
        const match = pattern.exec(message);
        if (match) { translated = replacement(...match.slice(1)); break; }
      }
    }
  }
  return interpolate(JAPANESE_DISCORD_MESSAGES["error.request"], { message: translated });
}
