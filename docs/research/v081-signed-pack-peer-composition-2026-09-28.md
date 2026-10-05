# v0.8.1 실제 서명 pack의 bounded peer·BuildUp 합성 검증

## 범위와 권위

기준 소스는 후보 브랜치 `codex/v081-selective-source-ci-20260927`의
`0d268bca2323e695b876ead3630faac3a071d66e`다. 해당 소스의 비게시 CI
[`36331512063`](https://github.com/daejunnom/Clearra/actions/runs/36331512063)는
Core·native products·WASM ABI·surfaces 네 작업 모두 성공했다. 이 성공을
아래 새 변경의 CI 또는 릴리스 성공으로 확대하지 않는다.

본 단계는 기능 연결과 집중 정확성 검증이다. 벤치마크, ABBA, P7P4 실행,
자산 재생성·전수 재자격, v0.9.0 TB 업그레이드, main 병합, 4194 변경 및 배포를
진행하지 않는다. 기존 혼합/primary 작업트리도 수정하지 않는다.

## 실제 자료로 드러난 seed 회귀

기존 peer 단위 테스트의 작은 closed relation은 삭제 행 없는 frame만 사용했다.
실제 catalog의 signed relation에는 2L의 삭제 원행 frame도 포함되어 있다.
그 pack은 정상적인 parser와 signature qualification을 통과했지만
`QualifiedRelationPeer::from_trusted_seed`에서 `InvalidWire`로 거절됐다.

원인은 seed의 context dependency union을 surviving row 개수로 제한한 검사다.
실제 점유는 compacted physical board에만 존재하지만, profile별 ordered kick의
충돌 의존 셀은 target 높이의 collision frame 안에서 그 위의 known-empty 셀도
참조할 수 있다. 유효한 마스크를 physical occupancy domain으로 검사하면 안 된다.

`decode_seed`는 마스크가 `width × target height` 안에 있는지 검사하도록 수정했다.
`record_is_canonical`, prepared context의 `accepts_board`, 실제 점유의 surviving-row
검사는 그대로 유지한다. 2L 한 행 삭제의 원시 생성 레코드를 이용한 회귀 테스트는:

- 유효한 virtual-empty dependency가 있는 seed를 받아들인다.
- 그 virtual row에 점유 비트가 있는 질의는 `OutOfScope`로 거절한다.
- target grid 밖 마스크는 checksum을 다시 계산해도 `InvalidWire`로 거절한다.

wire schema, profile 규칙, 자산 바이트·generation·서명 catalog는 변경하지 않았다.

## solver 연결 검증

새 test-only transport는 실제 seed/query/reply codec과 bounded cache를 사용하되
프로세스 전체 registry를 변경하지 않는다. production peer method를 공개하거나
solver hot loop에 테스트 코드·타이머를 넣지 않는다.

작은 synthetic fixture는 test-only authority이고 production 자격을 부여하지 않는다.
이를 이용해 즉시 cold miss, warm closed relation, partial Boolean positive 이후 다른
lock 조회, 미지원 원행 frame, good reply 이후 corrupt reply의 InvalidAsset/fallback을
실제 `ReachabilityWorkspace`에서 검증한다. 사후 전체 결과 무효화는 worker host의
별도 책임이며 이 Core 검증만으로 기존 partial의 product readback을 닫지 않는다.

실제 자료는 기존 immutable Release `conditioned-data-v081-20260924-rc1`의 다섯
`conditioned-<profile>.cllr` 파일만 명시적으로 받아 재사용한다. source-controlled
product catalog의 opaque authority와 실제 payload 길이·digest·generation·rule
binding을 기존 product qualifier로 검증한다. 다운로드 위치는 관리자가 확인한
후보 worktree의 `_local/artifacts/v081-peer-signed-smoke`이며 Git에 추가하지 않는다.

각 SRS/SRS+/SRS-X/Jstris 180/no-kick profile에서 지원되는 56개 context에 대해
점유 `0`과 `17`을 사용한다. 따라서 560개 profile/context/board 사례를 검사한다.
각 사례의 full owner, cold peer fallback, reply 뒤 peer의 Boolean shortcut on/off가
동일한 독립 primitive의 전체 lock-anchor 집합과 일치한다. warm peer의 실제
complete hit도 확인한다. 높이는 1~6L, 미노는 일곱 종류이며 삭제 frame은 기존
pack에 포함된 2L 원행 삭제 두 종류다. 모든 가능한 초기 필드·frame의 전수 증거나
PC/Build product reducer·witness·spin·finesse·multiplicity 최종 parity는 아니다.

## 관측 결과

| 검사 | 결과 | 권위의 한계 |
| --- | --- | --- |
| 새 peer/solver 및 virtual dependency 회귀 | 4개 통과 | 작은 synthetic fixture의 집중 증거 |
| Core 조건부 테스트 | 50개 통과, signed smoke 1개는 일반 실행에서 ignored | 자산 재생성·전수 재자격 아님 |
| 도달성 테스트 | 29개 통과, 2개 ignored | signed smoke는 별도로 명시 실행; 과거 generated candidate 전수 테스트는 재실행하지 않음 |
| signed five-profile owner/peer smoke | 1개 명시 테스트 통과, 560개 사례 | native codec/solver 증거; 실제 여러 WASM realm의 브라우저 실행 아님 |
| 비게시 CI 계약 | Node 6개 통과 | 새 CI의 성공을 대신하지 않음 |
| formatting·diff whitespace | 통과 | runtime/release authority 아님 |
| strict Clippy | 미통과 | Rust 1.98의 기존 pc-graph 경고 및 Core 기존 15개 lint에 차단; unrelated cosmetic 변경/경고 억제하지 않음 |

실제 signed smoke 감독 영수증은 `1790526149822589400-40836-runtime.json`이다.
조건부와 도달성 재실행은 각각 `1790526392113341600-41364-runtime.json`,
`1790526405293389000-11760-runtime.json`이다. 이 세 실행은 동일한 새 test executable을
직접 감독했으며 결과와 owned-tree 정상 종료를 별도로 확인한다. 이는 메모리
admission/종료 영수증이지 active-session asset peak 128MiB 증명 또는 성능 기록이 아니다.

앞선 Cargo compile+test 실행은 테스트가 통과해도 finite descendant drain이
`E_CLEARRA_PROCESS_TREE_NOT_STOPPED`로 끝났다. 해당 감독 경고를 정상 종료로
고쳐 적지 않는다. 감독 한도·resources를 바꾸거나 외부 프로세스를 종료하지 않았다.
위 직접 test-executable 실행의 정상 감독 결과와 Cargo tree 경고를 분리한다.

## 다음 경계

비게시 CI에 기존 qualified pack 다운로드와 같은 signed smoke를 추가한다. 앞 단계와
동일한 Core feature 집합을 사용하여 test executable을 다시 빌드하지 않는다.
signature/실제 pack 연결을 synthetic 테스트로 대체하지 않고 source 변경 때마다
검사한다. 새로운 qualification receipt나 release tag는 생성하지 않는다.

실제 WASM worker 분산 hit/fallback·취소/교체, PC/Build reducer와 모든 제품 surface의
전체 결과 parity, strict-lint 정리, peak/성능 gate 및 exact-SHA release는 계속 Open이다.
벤치마크는 사용자가 정한 다음 단계에서 진행한다.
