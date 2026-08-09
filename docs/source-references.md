# Clearra Source Reference Index

이 문서는 Clearra 설계와 검증 과정에서 참고한 로컬 문서, 외부 소스,
제품 문서를 한곳에 연결하는 온보딩 색인이다. 구현 완료표나 벤치마크
기록이 아니며, 외부 프로젝트를 Clearra의 정확성 근거로 승격하지 않는다.

## 읽는 순서와 우선순위

다른 작업자가 코드를 수정하기 전에는 다음 순서로 계약을 확인한다.

1. [`Clearra 핸드오프.md`](../Clearra%20핸드오프.md): 목표 구조, 정확성
   계약, 책임 경계, 명시적으로 반려한 설계를 정의한다.
2. 기능별 규범 문서: 알고리즘의 필요조건, fallback, 출력 의미를
   구체화한다.
3. contract test와 typed fixture: 실행 가능한 동작 경계를 고정한다.
4. 현재 소스: 위 계약을 구현하는 수단이다. 현재 코드가 목표 계약과
   다르다는 이유만으로 핸드오프 의미를 조용히 낮추지 않는다.
5. 외부 소스: 아이디어, 상호운용성, 독립 검증 자료다. 외부 결과나
   이름만으로 candidate를 제거하거나 complete 결과를 주장할 수 없다.

외부 구현을 직접 옮길 때에는 commit을 고정하고, license와 provenance를
기록하며, Clearra의 board, supply, hold, kick, spin, line-clear, identity,
completeness 계약으로 다시 검증한다. hash는 비교용 색인일 뿐 exact proof가
아니다.

## 로컬 소유권 지도

| 관심사 | 먼저 읽을 문서 | 주요 소스 경계 |
| --- | --- | --- |
| 전체 구조와 정확성 | [`Clearra 핸드오프.md`](../Clearra%20핸드오프.md), [`algorithm-policy.md`](algorithm-policy.md), [`pruning-policy.md`](pruning-policy.md) | `crates/clearra-problem`, `crates/clearra-core-executor`, `core-c` |
| inverse lock-clear와 geometry family | [`algorithms.md`](algorithms.md), [`buildup.md`](buildup.md), [`memory-lifecycle.md`](memory-lifecycle.md) | `crates/clearra-core-executor/src/backend/wasm_cpu`, `core-c/src/packing`, `core-c/src/buildup` |
| 공급, queue, hold, coverage | [`buildup.md`](buildup.md), [`build-coverage.md`](build-coverage.md), [`output-formats.md`](output-formats.md) | `crates/clearra-supply`, `crates/clearra-coverage`, `crates/clearra-build-coverage` |
| PC 셋업과 QB/미래 정보 | [`setup-search.md`](setup-search.md) | `crates/clearra-setup-search`, `crates/clearra-core-executor/src/backend/wasm_cpu/setup_*` |
| 선택적 PC4 tablebase | 핸드오프의 tablebase 계약 | `crates/clearra-core-executor/src/backend/wasm_cpu/pc4_tablebase.rs`, `crates/clearra-app/src/tablebase_runtime.rs`, `apps/clearra-web/src/workers/pc4TablebaseAssets.ts` |
| CPU/WebGPU 실행과 자원 | [`gpu-pipeline.md`](gpu-pipeline.md), [`runtime-budgets.md`](runtime-budgets.md), [`memory-lifecycle.md`](memory-lifecycle.md) | `crates/clearra-core-executor/src/cpu_worker_pool.rs`, `crates/clearra-webgpu`, `apps/clearra-web/src/workers` |
| rule, kick, spin, score | [`rules-and-kicks.md`](rules-and-kicks.md), [`scoring.md`](scoring.md), [`scoring-profiles.md`](scoring-profiles.md) | `crates/clearra-rules`, `crates/clearra-spin`, `crates/clearra-scoring`, `crates/clearra-forward-search` |
| CTK3/Fumen과 렌더링 | [`ctk3.md`](ctk3.md), [`output-formats.md`](output-formats.md) | `packages/ctk3`, `crates/clearra-fumen`, `crates/clearra-render`, `packages/clearra-ui/src/lib/workspace/ctk3*` |
| 제품 API와 GUI | [`app-boundary.md`](app-boundary.md), [`gui.md`](gui.md), [`gui-host.md`](gui-host.md), [`i18n.md`](i18n.md) | `crates/clearra-app`, `crates/clearra-gui-host`, `packages/clearra-ui`, `apps/clearra-web` |
| Clearrabot Oracle Gateway ingress, 원격 Cloud Run job | [`apps/clearra-discord-bot/README.md`](../apps/clearra-discord-bot/README.md), [`CLOUD_RUN_JOB_SERVICE.md`](../apps/clearra-discord-bot/CLOUD_RUN_JOB_SERVICE.md) | `apps/clearra-discord-bot/src/clearra`, `apps/clearra-discord-bot/src/discord`, `apps/clearra-discord-bot/src/ingress`, `apps/clearra-discord-bot/src/job-service` |
| 외부 PC fixture | [`external-pc-fixtures.md`](external-pc-fixtures.md), [`source_registry.json`](../tests/fixtures/external-pc/source_registry.json) | `tests/fixtures/external-pc` |

`docs/research`의 문서는 특정 실험과 채택/반려 근거를 보존한다. 수치와
artifact hash는 연구 기록이며 장기 제품 계약이 아니다. 같은 최적화를 다시
시도하기 전에는 다음 문서를 먼저 확인한다.

- [`sfinder-command-role-audit.md`](research/sfinder-command-role-audit.md)
- [`build-probability-optimization.md`](research/build-probability-optimization.md)
- [`suffix-sharing-optimization.md`](research/suffix-sharing-optimization.md)
- [`forward-spin-optimization.md`](research/forward-spin-optimization.md)
- [`spin-structure-search-2026-08.md`](research/spin-structure-search-2026-08.md)
- [`player-gui-clean-room-2026-08.md`](research/player-gui-clean-room-2026-08.md)
- [`tsar-half-geometry-bottleneck.md`](research/tsar-half-geometry-bottleneck.md)

대화에 첨부되었던 `APDP English.md`, `APDP 한국어.md`, 역방향 탐색
핸드오프와 setup-finder 기획 메모는 설계 입력 자료다. repository 밖의 개인
다운로드 경로를 build/document dependency로 만들지 않는다. 채택한 APDP,
parity, bumper, separator/MITM, setup family-quotient 계약은 루트 핸드오프와
기능별 문서에 완전히 다시 적어 두어야 한다.

## PC 탐색과 외부 solver

### knewjade/solution-finder

- Repository: <https://github.com/knewjade/solution-finder>
- Clearra audit 기준 commit:
  [`0e7c935a5399159a3d9c42fb8721e3c6842ae17d`](https://github.com/knewjade/solution-finder/commit/0e7c935a5399159a3d9c42fb8721e3c6842ae17d)
- 기준 제품 버전: `1.43`.
- `1.43` 구조 탐색 이식 감사 기준 commit:
  [`e8b291b47702cd08daf982bd52ef946902354848`](https://github.com/knewjade/solution-finder/commit/e8b291b47702cd08daf982bd52ef946902354848).
- License: MIT (`Copyright (c) 2020 knewjade`). 언어만 바꾼 실질적 포트도
  원 저작권·허가문과 provenance를 보존하며, 포함된 Apache Commons CLI의
  attribution을 별도로 유지한다.
- 사용한 범위: command별 결과 계약, perfect packing, BuildUp, fixed-queue
  operation 선택, property kick format, Fumen 기반 입출력 관례. 독립된
  unordered-inventory spin structure 이식 경계는
  `crates/clearra-spin-structure-search/`에 한정한다.
- Clearra 경계: `percent`, `path`, `cover`, `setup`, `spin`은 서로 다른
  retained-evidence 계약으로 해석한다. strip/profile packing, DFS, I 전용
  경로, SRS 전용 가정은 Clearra의 inverse lock-clear exact-cover 주력
  backend로 복제하지 않는다.
- 상세 결정: [`sfinder-command-role-audit.md`](research/sfinder-command-role-audit.md),
  [`spin-structure-search-2026-08.md`](research/spin-structure-search-2026-08.md)

### wirelyre/tetra-tools

- Repository: <https://github.com/wirelyre/tetra-tools>
- Browser PC solver: <https://wirelyre.github.io/tetra-tools/pc-solver.html>
- Jstris cross-check 기준 commit:
  [`2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c`](https://github.com/wirelyre/tetra-tools/commit/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c)
- 사용한 범위: 4L P7P3/P7P4 독립 결과 비교, operation equivalence,
  queue-pattern UX, Jstris 180 table 확인, 브라우저 solver 성능 구조 비교.
- Clearra 경계: tetra-tools의 결과는 fixture/oracle 비교 자료다. 줄 삭제,
  hold provenance, concrete realization, coverage identity를 생략한 표현을
  제품 결과로 그대로 가져오지 않는다.

### randomidiot13/hydra

- Repository: <https://github.com/randomidiot13/hydra>
- 사용한 범위: 미리 계산된 PC 정보, compact lookup, tablebase 생성과
  runtime 조회의 분리 아이디어.
- Clearra 경계: tablebase는 선택적 exact accelerator다. artifact가 없거나
  identity가 맞지 않으면 일반 exact search를 실행하며, partial/unknown
  entry는 prune 권한이 없다.

### muse918/pcanalyzer-web

- Repository: <https://github.com/muse918/pcanalyzer-web>
- 사용한 범위: PC 분석 table 구조, policy/tablebase UX, 웹 lazy loading.
- 권한 메모: 작성자에게 코드 사용 허락을 받았다. 직접 사용하더라도
  Clearra의 schema, exact identity, license/provenance, fallback 계약으로
  감싸며 원본 구조를 제품 의미론으로 간주하지 않는다.

## Bot과 명령 호환성

### swng/sfinderbot

- Repository: <https://github.com/swng/sfinderbot>
- 사용한 범위: Discord command vocabulary, `allspin_pres_chance` 계열
  coverage 계산과 응답 형식 비교.
- Clearra 경계: bot script의 확률은 독립 진단 자료다. Clearra의
  `PatternBitSet` universe, buildability, B2B/spin profile과 다시 대조한다.

### cringemoment/sfinder-man

- Repository: <https://github.com/cringemoment/sfinder-man>
- Clearra audit 기준 commit:
  [`438187b6a0ce4bf543ffc9faae507fdc11970e13`](https://github.com/cringemoment/sfinder-man/commit/438187b6a0ce4bf543ffc9faae507fdc11970e13)
- 해당 commit에는 루트 `LICENSE`/`COPYING`이 없고, 포함된 `sfinder.jar`의
  버전·provenance도 고정되지 않았다. sfinder-man 고유 코드·데이터는 직접
  복사하거나 번역하지 않고 공개된 동작 계약만 독립 구현한다.
- 사용한 범위: 공개 command 목록, path/percent/cover/setup 역할, Discord UX와
  실패 응답. 원본의 공통 `asyncio.wait_for` 값은 1000초이지만 timeout 오류
  문구는 일관되게 3분이라고 안내한다. ClearraBot은 이 모순된 구현 상수를
  그대로 복사하지 않고 비방향 기타 호환 작업에 공개된 3분 계약을
  기본값으로 사용한다. 표현된 역방향은 5분, Sfinder-man 호환을 포함한
  정방향·셋업 탐색은 15분 정책을 적용하고 Discord 전달은 별도 안전 상한을
  둔다.
- Clearra 경계: 같은 이름이 다른 의미를 가지는 명령은 `clearra sfinder`
  compatibility namespace에 격리한다. Java subprocess나 Sfinder runtime을
  제품 backend로 사용하지 않는다.

### hsohliyt105/fumen-bot

- Repository: <https://github.com/hsohliyt105/fumen-bot>
- 사용한 범위: Fumen 입력의 별도 이미지 응답과 GIF viewer UX.
- Clearra 경계: renderer, GIF encoder, CTK3/Fumen decoder는 Clearra가
  소유한다. repository 복제나 외부 렌더링 service 호출을 제품 경로에
  넣지 않는다.

## Queue 문법과 셋업 정책

- Solution Finder guide: <https://hsterts.github.io/h-docs/sfinder/>
- Pattern syntax guide:
  <https://hsterts.github.io/h-docs/sfinder/parameter-patterns/>
- TX 5th best-saves sheet:
  <https://docs.google.com/spreadsheets/d/1PBUTcjoS4g7PIB5qTjoxyF8IY17eucSn90aUubQXDv4/edit?gid=1881467979#gid=1881467979>

이 자료들은 pattern 문법, initial hold 설명, 보이는 queue에 따라 setup을
선택하는 QB 정책을 이해하는 데 사용했다. Clearra의 문법은 호환 가능한
부분을 제공하되 동일 문법을 주장하지 않는다. QB 관측 미노는 사용 가능한
공급이며 전부 lock해야 하는 의무가 아니다. setup의 Build/Joint/Conditional
확률은 exact product state에서 계산한다.

## 독립 PC fixture와 해법 자료

- PC Info Korea: <https://sites.google.com/view/pcinfokorea/연속퍼클-정보/1회차/초급-셋업>
- FOUR PC opener: <https://four.lol/perfect-clears/opener/>
- Tsar Cannon source: <https://hse30.tistory.com/1224>

이 링크의 source metadata와 typed fixture 연결은
[`source_registry.json`](../tests/fixtures/external-pc/source_registry.json)이
소유한다. 외부 이미지 자체를 golden으로 복제하지 않고, 사람이 확인한
board/Fumen/solution identity를 저장한다.

## Rule, kick, spin, score 자료

- Jstris product: <https://jstris.jezevec10.com/?mode=1&play=1>
- TETR.IO official patch notes: <https://tetr.io/about/patchnotes/>
- TETR.IO rules summary: <https://tetris.wiki/Tetr.io>
- TETR.IO mechanics: <https://tetrio.wiki.gg/wiki/Mechanics>
- Tetra League: <https://tetrio.wiki.gg/wiki/TETRA_LEAGUE>
- Polymer T-spin notes: <https://pensil.wiki/wiki/polymer_tspin>
- Damage calculator: <https://pensil.wiki/tools/calculator>

공식 patch note와 verified kick table을 우선하고, community wiki와
calculator는 fixture와 해석 보조 자료로 사용한다. kick table,
spin-recognition profile, score table, attack table은 서로 다른 객체다.
특정 게임의 score/attack 규칙을 PC geometry prune으로 사용하지 않는다.

## Fumen과 CTK3 상호운용성

- Hard Drop Fumen viewer: <https://harddrop.com/fumen/>
- knewjade/tetris-fumen: <https://github.com/knewjade/tetris-fumen>
- npm `tetris-fumen`: <https://www.npmjs.com/package/tetris-fumen>

Fumen은 `v115@` compatibility와 page/comment/operation 동작의 독립 비교
대상이다. CTK3의 규범 문서는 [`ctk3.md`](ctk3.md), 구현 package는
`packages/ctk3`다. CTK3 canonical text는 고정 Base64url 계열 encoding을
사용하고, 다중 page, 색, comment, operation, file attachment를 보존한다.
encode마다 여러 alphabet을 시험해 가장 짧은 값을 고르는 정책은 사용하지
않는다.

## Web, worker, 배포 표면

- Emscripten pthreads:
  <https://emscripten.org/docs/porting/pthreads.html>
- WebGPU specification: <https://www.w3.org/TR/webgpu/>
- WebAssembly JavaScript API: <https://developer.mozilla.org/docs/WebAssembly/JavaScript_interface>
- SharedArrayBuffer: <https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer>
- SvelteKit static adapter: <https://svelte.dev/docs/kit/adapter-static>
- Tauri: <https://tauri.app/>
- Discord interactions: <https://discord.com/developers/docs/interactions/overview>
- Cloud Run request timeout: <https://cloud.google.com/run/docs/configuring/request-timeout>
- Cloud Run billing and CPU allocation: <https://cloud.google.com/run/docs/configuring/billing-settings>
- Cloud Run minimum instances: <https://cloud.google.com/run/docs/configuring/min-instances>
- Cloud Run concurrency: <https://cloud.google.com/run/docs/configuring/concurrency>
- GitHub Pages: <https://docs.github.com/pages>

이 자료는 worker, shared memory, WebGPU security initialization, static SPA,
signed interaction, timeout, deployment 제약을 확인하는 데 사용한다. 플랫폼
제약은 solver의 complete/incomplete 의미를 바꾸지 않는다. 현재 Discord
slash 경로는 Cloud Run 컨테이너 안의 direct executor에 typed argument
array를 전달한다. 명시적으로 설정한 원격 job runner만 idempotent job
contract를 사용하며, 어느 경로도 shell text를 신뢰 경계로 삼지 않는다.

## 명시적으로 채택하지 않은 외부 의존 방식

- TinyURL 또는 별도 2단계 document 저장 service를 필수 제품 경로로 두지
  않는다. 짧은 값은 Clearra viewer URL을 사용하고, 긴 값은 canonical
  `.ctk3` attachment로 전달한다.
- 외부 solver process, Java runtime, 외부 renderer service를 silent
  fallback으로 실행하지 않는다.
- 외부 결과 수, bot 응답, screenshot, wiki 문구만으로 exact candidate를
  제거하지 않는다.
- unpinned repository의 현재 branch를 재현 가능한 알고리즘 출처로
  인용하지 않는다. 코드 이식이 필요하면 먼저 commit을 고정한다.
