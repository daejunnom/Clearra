# BuildUp 가속 미해결 문제 대장

> 작성일: 2026-09-22
> 점검 기준: `codex/converge-v081-v090-performance-20260921` / `dbc2140213214f96f8c705a5237441dc008ca5c2`.
> 상태: 문서화만 수행. 아래 항목을 수정 완료나 릴리즈 게이트 실패 원인으로 표시하지 않는다.
> 관련: [현재 기획](compact-board-conditioned-reachability-plan.md), [검증 계획](build-up-acceleration-validation-plan.md).

> **현재 상태 오버레이.** 이 문서 아래의 원문 관측과 재현식은 역사·증거로
> 보존한다. 현재 구현 브랜치에는 LB-001~004의 typed codec, 상태 분리,
> completion capability, 확장 fingerprint가 구현됐고 집중 테스트가 통과했다.
> 이는 실제 다섯 profile 자산의 exhaustive qualification, 성능 채택 또는 릴리스
> 완료가 아니다. 최신 완료 권위는 상위 통합 계획의 `현재 구현 스냅샷`을 따른다.

## 1. 상태와 근거의 의미

- **소스 관측**: 고정 소스의 제어 흐름/데이터 계약에서 확인한 사실. 실제 제품 오류 재현과는 다르다.
- **설계 위험**: 새 자산을 연결할 때 방지할 조건. 현재 기본 제품에 동일한 버그가 존재한다는 뜻이 아니다.
- **사용자 정정**: 이번 기획에서 반영해야 하는 명시적 요구.
- **미해결**: 문서 작업으로 코드가 바뀌지 않았고 종결 증거가 없다.

`LocalPc4LegalBoardIndex`와 관련 setter는 `local-search-ab`에 제한되어 있고,
확인한 기본 정책의 `legal_board`는 false다. 아래 로컬 필터 문제를 모든 제품 실행의
활성 오류로 일반화하지 않는다. 재현 여부와 영향 범위는 각 항목에서 별도로 추적한다.

## 2. 요약

| ID | 항목 | 분류 | 상태 |
| --- | --- | --- | --- |
| LB-001 | 생성기/소비자 삭제 행 정규화 불일치 | 소스 관측 | 구현·집중 검증 완료, 실제 자산 parity 미완료 |
| LB-002 | 중간 layer의 완전성·미적재 상태를 설치 API가 표현하지 않음 | 소스 관측 | 구현·집중 검증 완료, 제품 smoke 미완료 |
| LB-003 | PC 필터 호출에 완료 목표가 전달되지 않음 | 소스 관측/연결 위험 | 구현·집중 검증 완료, 전체 differential 미완료 |
| LB-004 | 현재 domain fingerprint와 강화된 도달성 자산의 의미 범위 차이 | 소스 관측/설계 위험 | 구현·집중 검증 완료, production generation 미완료 |
| LB-005 | GB급·수백 MB급 자산을 lazy라는 이유로 재도입 | 사용자 정정 | 해당 주력안 철회 |
| LB-006 | 고정 킥 후보와 보드 조건부 실제 전이를 혼동 | 사용자 정정/설명 정정 | 후속 설계에 반영, 구현 없음 |
| LB-007 | 국소 관계를 전체 spawn-to-lock 증명으로 확대 | 설계 위험 | 전체 보드 충돌 기준의 entry→first-exit/lock 원시 관계·독립 레코드 감사·별도 unqualified 후보 생성기 소스; 완전 domain, 서명 pack, 전역 합성, Rust 실행 검증 미해결 |
| LB-008 | 목표 도달 가능 보드와 특정 도면의 구축 가능성을 혼동 | 설계 위험 | 미해결 |
| LB-009 | first-success의 보드 변경을 단조 변화로 취급 | 설계 위험 | 미해결 |
| LB-010 | 순서 언어/대표 lock의 공유로 실현·개수·스핀 증거 손실 | 설계 위험 | 미해결 |
| LB-011 | generation·캐시·검증 상태 혼합 | 설계 위험 | 미해결 |
| LB-012 | 그래프 생성 뒤의 언어 캐시를 BuildUp 전체 생략으로 오인 | 소스 관측/성능 해석 | 미해결 |

## 3. LB-001 — 삭제 행 정규화 불일치

**관측.** `pc4_graph_materializer.rs`는 지워진 행을 꽉 찬 하단 prefix로 정규화한다.
`LocalPc4LegalBoardIndex`의 설명도 같은 형식을 말한다. 하지만
`local_pc4_legal_board_allows()`는 `deleted_rows`가 가리키는 원래 목표 행 위치에
full row를 삽입한다. 로컬 테스트도 non-prefix full row를 수동 목록에 넣는다.

소스: [materializer][materializer], [local policy][policy].

동일한 입력에 대한 두 표현의 재현 산식은 다음과 같다.

```python
width, height = 10, 4
board = 0b11 << 20
deleted_rows = 1 << 2
row_mask = (1 << width) - 1

consumer_key = 0
physical_row = 0
for target_row in range(height):
    if deleted_rows & (1 << target_row):
        row = row_mask
    else:
        row = (board >> (physical_row * width)) & row_mask
        physical_row += 1
    consumer_key |= row << (target_row * width)

cleared_count = deleted_rows.bit_count()
prefix_key = (board << (width * cleared_count)) | ((1 << (width * cleared_count)) - 1)
assert consumer_key == 0x00fff00000
assert prefix_key == 0x00c00003ff
assert consumer_key != prefix_key
assert consumer_key.bit_count() == prefix_key.bit_count() == 12
```

이는 좌표 식의 반례이며, 그 보드가 실제 생성 domain에 속한다거나 제품의 해법 누락을
재현했다는 증거는 아니다. 면적 검사만으로 표현 불일치를 잡지 못함을 보여준다.

**수정 방향.** 물리 조회 key와 ILC의 원래 행 대응을 별도 타입으로 둔다.
생성기와 소비자가 공유하는 정확한 codec 계약을 정하고 실제 생성 fixture를 연결한다.
**종결 조건.** 삭제 행 전체 조합, 중간 행 삭제, 다중 삭제, 실제 reachable 보드에서
round-trip과 전체 도면/coverage 일치 검사가 통과해야 한다.

## 4. LB-002 — 미적재와 검증된 부재의 구분 부족

**관측.** `LocalPc4LegalBoardIndex::new()`는 각 원소의 면적, layer 0의 empty,
layer 10의 full seed를 확인한다. 중간 layer가 완전히 생성/전달되었다는 증명은
설치 인자에 없다. `contains()` 실패는 필터에서 거절로 이어진다.

소스: [local policy][policy].

따라서 미생성/미다운로드 layer를 빈 벡터로 전달하면, 그 layer가 완전히 비었다는
의미와 구분할 수 없다. 기존 호출자가 완전한 파일만 전달한다면 위험을 외부에서
막을 수 있으나 설치 API 자체가 이를 강제하지 않는다.

**수정 방향.** `LoadedComplete`, `NotLoaded`, `OutOfScope`, `InvalidAsset` 및
검증된 negative record/완전 domain의 부재를 구분한다. 부분 자산의 miss는 Unknown이다.
**종결 조건.** 미적재, 잘린 파일, 잘못된 인덱스, 빈 완전 layer, 정상 negative를
각각 검사한다. complete라는 선언만이 아니라 생성 범위와 검증 manifest를 확인한다.

## 5. LB-003 — 완료 목표의 적용 범위 누락

**관측.** `BuildOrderGraph::build()`는 `completion`을 받는다. 그 안의 로컬 PC4
필터 호출에는 width, height, initial board, kick profile, physical board,
deleted rows, depth만 전달되고 완료 목표는 전달되지 않는다.

소스: [buildup.rs][build], [local policy][policy].

일반 구축 목표까지 해당 로컬 정책이 활성화되는 경로가 있다면,
PC로 이어지지 않는다는 사실로 정상적인 목표 필드 구축을 거절할 수 있다.
이번 문서 작업에서 해당 잘못된 결과의 종단 실행은 재현하지 않았다.

**수정 방향.** 목표 독립 물리 도달 관계와 목표 종속 co-reachability를 별도 capability로 둔다.
PC 필터에는 명시적 목표 계약을 요구한다.
**종결 조건.** PC, 일반 Build, 중간 접속 목표/경계 커버리지, 정확 높이와 높이 상한을
교차 검사하고 범위 밖에서는 기준 경로와 같은 결과를 얻는다.

## 6. LB-004 — 의미 fingerprint 범위

**관측.** `DomainBinding::legal_board()`의 digest는 domain/version 문자열,
10×4 선언, profile명, 미노/회전 구분, 순서 있는 킥 오프셋을 포함한다.
강화된 도달 관계의 결과에는 미노 shape 원점, 시작 pose 집합, ceiling,
벽·바닥·lock·행 삭제·정규화 규칙도 영향을 준다.

소스: [domain.rs][domain], [reachability.rs][reach].

현재 digest가 이미 충돌했다거나 기존 자산이 틀렸다는 관측은 아니다.
사전 기록의 권한을 필터에서 실제 물리 계산 대체로 확대할 때의 호환성 요구다.

**수정 방향.** 지원 domain과 의미 버전을 모두 묶고, 바이트 hash와 의미 호환성을 구분한다.
**종결 조건.** 각 의미 항목을 하나씩 바꾼 변이에서 잘못된 재사용이 차단되는지 검사한다.

## 7. LB-005 — 대형 TB의 재도입

**사용자 정정.** 기존 약 600MB PC4 TB도 온라인 조회 전용으로 바꾸었으므로,
GB급 전 보드 lock 테이블·무제한 도면 언어 테이블은 이번 기능의 주력안이 아니다.
이 크기는 사용자가 제공한 기존 운용 맥락이며 이번 작업의 실측값이 아니다.

사전 생성 시간이 무관하다는 이유로 배포 용량을 무관하게 취급한 이전 기획은 철회한다.
샤딩, 사전화, lazy 다운로드가 실제 총량 감소의 증거는 아니다.

**수정 방향.** 주변 보드 조건의 공유 관계를 소형 자산으로 만든다.
총 압축/해제 크기, 요청/세션 누적 수신, 상주/worker 복제 peak를 별도 budget으로 둔다.
**종결 조건.** budget 숫자가 명시되고 모든 크기 측정이 통과해야 한다.
기존 PC4 TB의 전체 또는 반복적 대량 조각 조회가 숨은 필수 의존성이면 미통과다.

## 8. LB-006 — 킥·도달성 설명 정정

**정정.** 킥 후보 오프셋의 순서는 프로필에 고정되지만,
실제 선택되는 킥과 도달 pose는 주위 보드의 충돌 상태에 따라 달라진다.
소스의 `first_successful_kick()`도 첫 collision-free target을 선택한다.

소스: [reachability.rs][reach].

저장할 것은 `미노+회전 → 한 킥 결과`가 아니라
`보드 의존 조건 + 진입 pose → 가능한 전이/도달 pose 관계`다.
뒤 후보의 목적지가 비어 있다는 이유만으로 앞 후보의 성공을 무시해서는 안 된다.
역방향 탐색도 정방향 first-success를 만족하는 source만 허용한다.

**종결 조건.** 앞 후보를 막는 한 셀 변경으로 결과가 바뀌는 사례,
여러 후보가 동시에 비어 있는 사례, 모든 후보 실패, 벽·바닥·180 회전,
역방향 후보 우선순위의 대조 검사가 통과해야 한다.

## 9. LB-007 — 국소 도달성과 전역 도달성

**위험.** 같은 목표 주변 모양이어도 스폰부터 그 영역으로 들어오는 통로가 다를 수 있다.
또한 창 밖의 앞선 kick 후보가 국소 first-success를 바꿀 수 있다.

**수정 방향.** 의존 셀과 경계/halo를 보존하고 입력을 실제로 도달한 진입 pose로 제한한다.
국소 출구를 전역 연결에 사용하며, 밖으로 나갔다 돌아오는 경로를 누락하지 않는다.
닫힌 범위가 아니면 국소 미발견으로 전역 불가능을 확정하지 않는다.

**종결 조건.** 같은 내부 점유·다른 외부 보드/진입 집합 쌍,
밖으로 우회한 뒤 들어오는 경로, 경계를 넘어가는 kick footprint를 검사한다.
임의의 '밖을 모두 비움/모두 채움' 두 실험만으로 일반 등가성을 증명했다고 하지 않는다.

**현재 구현 후보.** 전체 물리 보드에서 first-success를 평가하며 pose 창 안의
lock과 최초 창 밖 exit를 반환하는 원시 관계를 추가했다. 따라서 exit가 남으면
국소 lock 부재를 전역 음성 판정으로 승격하지 않는다. 독립 BFS로 exit 후속 결과를
합성하는 테스트는 소스에 있으나 실행 검증은 아직 없다. 전체 보드 충돌 판정이
읽을 수 있는 모든 창 내부 source·이동·선행 킥 후보 셀의 보수적 마스크를
in-process 재사용 조건으로 추가했지만, 압축된 재사용 pack과 제품 BuildUp 연결은
없다. 원시 shape·킥 해석만 쓰는 독립 국소 BFS로 첫 출구 집합을 직접
대조하는 테스트도 추가했지만 아직 실행하지 않았다. 후보 인덱스에서는 서로
겹치는 점유 조건에 다른 결과가 달린 경우를 사전 거절한다. 이 인덱스 역시
비권위 in-process 후보이고 검증된 제품 자산이 아니므로 항목은 계속 미해결이다.
별도 `CLLR0002` 후보 pack과 `qualification-reference` 전용 레코드 감사를
추가했다. 감사는 원시 shape와 ordered kick으로 모든 정적으로 유효한 창 내부
source 및 첫 후보까지의 **전체** 충돌 의존 셀을 재구성하고, 독립 BFS의
lock·exit와 저장된 값을 비교한다. compact physical board가 같더라도
삭제된 원래 행 위치가 다르면 국소 관계 후보의 키가 달라지도록 했다.
별도 비게시 source CI에서 국소 pack·독립 감사 테스트를 실행한다. 특히
local dependency 외부가 다른 보드에 relation을 재사용한 뒤 전역 출구를
합성할 때 저장 당시 보드가 아니라 실제 질의 보드를 사용하도록 경계를
수정했다. 이 테스트의 성공 여부는 해당 CI 결과로만 판정하며,
임의 보드·진입 집합에 대한 완전성, 외부 우회 합성, signed generation,
16MiB/128MiB 크기·상주량 및 BuildUp 제품 연결은 여전히 미해결이다.
기존 release gate가 다섯 profile의 sparse spawn-to-lock catalog만으로
통과할 수 있던 의미 혼동도 확인해, 국소 entry/first-exit 제품 계약이
실제로 연결되기 전에는 명시적 No-Go가 되도록 했다.

## 10. LB-008 — 보드·도면·목표의 세 가지 의미

어떤 보드가 PC로 이어질 수 있어도 현재 ILC가 지정한 도면으로 이어지는지는 별도다.
Geometry의 부분 배치 마스크는 실제 플레이 중간 보드가 아닐 수 있다.
빈 보드의 `F∩R` 밖에 있다는 이유로 임의 초기 필드의 해법을 거절할 수도 없다.

**수정 방향.** 물리 도달 관계는 현재 도면의 실현 검사에만 적용하고,
남은 operation과 목표 프레임의 셀 소유권을 별도 유지한다.
완전한 `R`은 일치하는 물리 domain에서만 negative 근거가 될 수 있다.

**종결 조건.** 같은 물리 보드/다른 잔여 도면, 다른 초기 블록,
여러 삭제 이력의 동일 점유 상태를 비교한다.

## 11. LB-009 — 장애물 변경의 비단조성

first-success 때문에 장애물을 추가하면 앞 후보가 실패하고 뒤 후보가 선택될 수 있다.
따라서 `B ⊆ B'`이면 `Reach(B') ⊆ Reach(B)`라는 단조성을 일반적으로 가정하지 않는다.
이 경고는 현재 제품에서 그런 잘못된 최적화를 발견했다는 뜻이 아니다.

**수정 방향.** 조건 서명 변경에서 간선 추가·삭제 양쪽을 갱신한다.
대체 경로와 순환을 포함하는 전역 reachability를 정확히 갱신하거나 기존 검색으로 재계산한다.
행 삭제 후의 좌표 이동도 새로운 snapshot/epoch에 포함한다.
**종결 조건.** 장애물 증감, first-success 교체, 우회 경로 복구,
삭제 후 많은 셀이 이동하는 입력을 검사한다.

## 12. LB-010 — 결과 공유의 정보 손실

같은 source/target이나 같은 미노 순서라도 실제 회전·lock·스핀 증거·실행 경로 수가 다를 수 있다.
후보 도면끼리 구축 언어를 공유해도 원본 도면 ID가 합쳐지는 것은 아니다.

소스에서 일반 언어, `CountAll`, finesse 처리 경로가 분리되어 있다: [buildup.rs][build].

**수정 방향.** Boolean 도달성, 전체 lock, witness, multiplicity, 스핀/finesse 지원을 명시한다.
지원하지 않는 증거는 기준 경로에서 생성한다. 후보 필수 포함/최소화/canonical ID를 보존한다.
**종결 조건.** 같은 endpoint의 여러 실현, 같은 언어의 여러 도면,
같은 lock의 다른 terminal action, 정상/스핀 양쪽이 가능한 사례를 교차 검사한다.

## 13. LB-011 — 자산·캐시의 세대 혼합

**위험.** 새 manifest와 옛 payload, 다른 profile 사전, 오래된 local relation을
동일 ID로 재사용하면 조용한 오판정이 가능하다. 해시 hit도 곧 exact key 일치는 아니다.

**수정 방향.** 실행 generation 고정, 규칙/정규화 fingerprint, exact key 검증,
부재/미적재/손상 구분, 해제 크기·참조 범위 검증을 요구한다.
상한을 넘긴 캐시의 eviction은 miss이며 누락 상태는 불가능이 아니다.
자산 실패 후 이미 수행한 잘못된 prune은 작업 재실행 또는 결과 무효화가 필요하다.

**종결 조건.** 세대/파일 교체, truncated 데이터, 잘못된 offset, digest 불일치,
동시 worker의 교체·eviction·취소, 부분 네트워크 실패를 검사한다.

## 14. LB-012 — 언어 캐시 hit의 성능 해석

**관측.** 일반 검증에서 `BuildOrderGraph::build()`가 먼저 실행되고,
그 뒤 `piece_order_languages.canonicalize()` 및 공급 커버리지 캐시가 사용된다.

소스: [buildup.rs][build].

따라서 언어 hit율이 높다는 사실만으로 그 앞의 도달성/그래프 생성이 생략되었다고 할 수 없다.
현재 `ReachabilityWorkspace`의 하드드롭/부분·전체 검색/캐시도 기준 비용에 포함해야 한다.

**수정 방향.** `CandidateProjection`, `BuildOrderReachability`, 언어 정규화,
`CoverageLanguageProduct`의 절감량을 분리 측정한다.
작은 조건부 전이표만으로 총 BuildUp이 가속되는지 확인하며, 부족하다고
GB급 전체 언어 TB로 자동 확대하지 않는다.
**종결 조건.** 동일 단계/종점의 A/B, 실제 생략 검색·그래프 연산 수,
콜드·웜 및 worker 복제 포함 측정이 남아야 한다.

## 15. 아직 확인하지 않은 것

- 신규 소형 자산의 생성 크기·압축률·호환성·도달성 정확성.
- 사용자 PC에서의 새 성능 또는 실제 해법 누락 종단 재현.
- 로컬 branch 외의 모든 제품 경로에 대한 영향 범위.
- 어떤 항목이 별도 v0.8.1 릴리즈 게이트 실패를 설명한다는 결론.

문제 대장은 기억 보존용 기록이지 실행 테스트 통과나 수정 완료 선언이 아니다.

[policy]: https://github.com/daejunnom/Clearra/blob/dbc2140213214f96f8c705a5237441dc008ca5c2/crates/clearra-core-executor/src/search_prune_policy.rs
[materializer]: https://github.com/daejunnom/Clearra/blob/dbc2140213214f96f8c705a5237441dc008ca5c2/crates/clearra-core-executor/src/backend/wasm_cpu/pc4_graph_materializer.rs
[domain]: https://github.com/daejunnom/Clearra/blob/dbc2140213214f96f8c705a5237441dc008ca5c2/tools/clearra-pc4-qualifier/src/domain.rs
[reach]: https://github.com/daejunnom/Clearra/blob/dbc2140213214f96f8c705a5237441dc008ca5c2/crates/clearra-core-executor/src/backend/wasm_cpu/reachability.rs
[build]: https://github.com/daejunnom/Clearra/blob/dbc2140213214f96f8c705a5237441dc008ca5c2/crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs
