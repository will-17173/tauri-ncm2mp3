# NCM Decoder Repair Implementation Plan

> Execute the steps inline in this session. The user's repository preferences exclude the brainstorming, TDD, worktree, and subagent-driven-development workflows.

**Goal:** Decode NCM files with reserved cover padding correctly in both the website and desktop app, and give the output its real audio extension.

**Architecture:** Keep the site's dependency-free decoder. Extract the Rust byte parser into `src-tauri/src/ncm.rs` so container validation and audio detection can be tested without starting Tauri. Both implementations read the cover frame capacity separately from the image length, skip the whole frame, and reject unrecognized decrypted data.

**Tech Stack:** JavaScript ES modules, Node's built-in test runner, Rust, existing AES/anyhow/serde_json dependencies, Vue/Vite/Tauri.

Reference for the container layout and metadata AES key: [ncmdump's implementation](https://github.com/taurusxin/ncmdump/blob/master/src/ncmcrypt.cpp).

## Task 1: Website decoder

Files: `site/ncm.js`, `site/tests/ncm.test.mjs`, `site/tests/fixtures/ncm-vectors.json`.

- [x] Replace the metadata key with `2331346C6A6B5F215C5D2630553C2728` and validate encrypted key alignment, PKCS#7 padding, and the nonempty `neteasecloudmusic` key prefix.
- [x] Read the complete cover header and validate its lengths before skipping its capacity:

```js
const coverFrameLength = view.getUint32(offset + 5, true);
const coverLength = view.getUint32(offset + 9, true);
offset += 13;
if (coverLength > coverFrameLength) {
  throw new Error('封面图片长度超过预留空间。');
}
if (coverFrameLength > data.length - offset) {
  throw new Error('封面数据段越界。');
}
offset += coverFrameLength;
```

- [x] Recognize FLAC, ID3/MP3 frames, Ogg, RIFF/WAVE, and M4A (`ftyp` at byte 4). Reject unknown audio instead of falling back to MP3 or trusting metadata.
- [x] Add fixed AES/keystream test vectors generated independently of the production decoder. Exercise a 7,442-byte empty cover frame, partially occupied frames, legacy frames without padding, zero-length frames, correct metadata decryption, every supported audio signature, malformed lengths/keys, and unknown audio.

Run: `node --test tests/ncm.test.mjs` inside `site/`. Expected: all cases pass.

## Task 2: Desktop decoder

Files: `app/src-tauri/src/ncm.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/tests/fixtures/ncm-vectors.json`, `app/README.md`.

- [x] Add a pure `decode_ncm(&[u8]) -> Result<DecodedNcm>` parser with checked section reads, strict AES/PKCS#7/key prefix validation, full cover frame skipping, and the same audio signatures as the website.
- [x] Keep file I/O in `convert_single_ncm`, deriving the extension from the decoded audio:

```rust
let decoded = ncm::decode_ncm(&fs::read(path)?)?;
let output_path = path.with_extension(decoded.format);
fs::write(&output_path, &decoded.audio)?;
```

- [x] Add Rust unit tests with the same fixed vectors for padded and legacy covers, supported formats, malformed sections/keys, and unknown audio. Add a file conversion test verifying both the real extension and exact output bytes.
- [x] Update the README to describe preservation of the original encoding and `.mp3`/`.flac` extensions.

Run: `cargo fmt --check` and `cargo test --offline` inside `app/src-tauri/`; `pnpm build` inside `app/`. Expected: formatting, tests, and the frontend build pass.

## Task 3: Verify real input and integration

- [x] Decode the supplied NCM locally with both implementations in temporary directories. Compare SHA-256 hashes against the previously verified FLAC; run `flac -t` on both outputs. Never overwrite the MP3 already delivered in Downloads.
- [x] Use the system-installed Playwright to verify the website's file selection, batch conversion, download filename/content, drag-and-drop, and invalid-file feedback.
- [x] Start `pnpm tauri dev` and check file/folder conversion, drag-and-drop, and logs when the native automation surface permits it. State any unavailable native interaction checks clearly.
- [x] Review the final changes and report the test results and remaining release/deployment boundaries. Do not commit, publish, or replace installer bundles as part of this source update.

## Verification results

- Website: 18/18 Node tests passed.
- Rust: 11/11 regular tests passed; the optional local-file test was also run separately and passed.
- Formatting and `pnpm build` passed; both repositories passed `git diff --check`.
- Both decoders produced the same fully verified FLAC for the supplied sample: SHA-256 `2a6c409283e1b2039b3a79e13b0920c46d7092ae699a11b1f8beb5cb83924c06`.
- Browser integration passed file selection, batch conversion, real FLAC download and hash, legacy MP3 download, drag-and-drop, and invalid-file feedback, with no page errors. Used the system Playwright installation and installed Chrome.
- `pnpm tauri dev` launched successfully. The current debug binary was temporarily wrapped in an app bundle outside the repository for native automation. Native file selection, recursive folder conversion, success/error logs, and output bytes passed. Native cross-window drag-and-drop was not verified; browser drag-and-drop was verified.
- Independent requesting-code-review review found no actionable issues.
- Source changes only; deployment and installer packaging were not requested.
