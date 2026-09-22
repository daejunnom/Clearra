# Wirelyre와 Clearra legal-board의 차이 및 크기 분석

## 1. 목적과 비교 기준

이 문서는 Wirelyre `tetra-tools`의 `legal-boards`가 만드는 최종 상태 집합과
Clearra의 현재 독립 legal-board 생성 경로가 만든 역방향 상태 집합을 구분한다.
목적은 다음 세 가지다.

1. 상태 수가 증가한 의미적 지점을 찾는다.
2. 같은 상태를 저장할 때 발생하는 직렬화 크기 차이를 분리한다.
3. GPL 구현을 복사하지 않고 Clearra의 정확한 profile별 제품으로 재설계할 경계를
   고정한다.

조사 기준일은 2026-09-22이며, 확인한 Wirelyre `main`은
[`2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c`](https://github.com/wirelyre/tetra-tools/tree/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c)이다.
주요 원본은 다음과 같다.

- [정방향 그래프 생성과 완성 필드에서의 역추적](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/legal-boards/src/boardgraph.rs)
- [정렬 차분 unsigned LEB128 직렬화](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/srs-4l/src/board_list.rs)
- [최종 `legal-boards.leb128` 작성](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/legal-boards/src/main.rs)
- [웹 Worker의 단일 자산 로드](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/gomen/worker.js)
- [GPL-3.0-or-later 라이선스](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/LICENSE)

비교 수치는 동일 규칙 결과의 parity 증거가 아니다. Wirelyre는 Jstris와 TETR.IO
placement의 합집합을 사용하고, Clearra 측정값은 정확한 SRS+ kick profile을
사용한다. 이 문서의 수치는 집합 정의와 저장 형식의 차이를 설명하는 진단
증거다.

## 2. 집합 정의

4L PC에서 k개 피스를 놓은 필드 집합을 다음처럼 정의한다.

- `F_k`: 빈 필드에서 정확한 회전·이동·lock 규칙으로 k개를 놓아 도달 가능한 필드
- `R_k`: 해당 필드에서 나머지 피스를 놓아 완성 필드까지 갈 수 있는 필드
- `L_k = F_k ∩ R_k`: 실제 legal-board 최종 집합

Wirelyre의 최종 자산은 `L_k`를 담는다. 정방향으로 `F_0`부터 `F_10`까지
전개하면서 각 도착 필드의 predecessor를 보존하고, 완성 필드에서 predecessor를
역추적해 `R_k` 조건까지 만족하는 필드만 남긴다.

Clearra의 현재 전용 실행기
[`clearra-pc4-legal-board.rs`](../../tools/clearra-pc4-qualifier/src/bin/clearra-pc4-legal-board.rs)는
`legal-board-run`에서 곧바로 `run_reverse_layers`를 호출한다. 따라서 현재 생성한
`reverse-layer-N.bin`은 `R_k`이며 최종 `L_k`가 아니다.

일반 qualifier의 `domain-run`에는 나중에 빈 필드부터 정방향으로 전개하고 해당
역방향 layer에 포함된 target만 유지하는 단계가 있다. 그러나 이 구성은 모든
`R_k`를 먼저 물질화해야 하므로 중간 집합과 디스크 사용량이 크게 증가한다. 전용
legal-board 실행기는 이 정방향 교집합 단계도 아직 수행하지 않는다.

## 3. 생성 알고리즘 차이

| 구분 | Wirelyre 원본 | Clearra 현재 적용 경로 | 영향 |
| --- | --- | --- | --- |
| 시작 방향 | 빈 필드에서 정방향 | 완성 필드에서 역방향 | Clearra가 빈 필드에서 도달 불가능한 co-reachable 상태까지 먼저 생성 |
| 최종 집합 | 정방향 그래프의 predecessor를 완성 필드에서 역추적한 `F_k ∩ R_k` | 현재 전용 실행기는 `R_k`까지만 생성 | 낮은 층에서 상태 수가 급증 |
| 전이 보존 | 도착 필드별 predecessor 보존 | `(source, piece)` 후보를 외부 run으로 spill한 뒤 ILC 재검증 | RAM은 제한되지만 임시 디스크 사용량 증가 |
| 조기 불능 판정 | isolated-cell과 imbalanced-split 제거 | 현재 reverse 생성 경로에는 동등한 조기 필터 없음 | 기하 후보와 ILC 검증량 증가 |
| 회전 중복 | O와 I/S/Z의 동일 모양 orientation을 canonical화 | 최종 target field에서 정렬·중복 제거 | 주로 전이 계산량 차이이며 최종 board 의미 차이는 아님 |
| 규칙 범위 | Jstris와 TETR.IO placement 합집합 한 자산 | SRS, SRS+, SRS-X, Jstris 180, no-kick의 정확한 개별 자산 | Clearra는 profile identity와 자산 수가 늘지만 profile 간 오염을 방지 |
| 결과 저장 | 모든 층을 하나로 정렬하고 delta ULEB128 | 층별 `PC4DOM02`, 128-byte header와 상태당 고정 `u64` | 같은 상태 수에서도 Clearra 파일이 더 큼 |
| provenance | 단일 공개 파일 | profile/kick identity, derivation, input/filter digest chain | Clearra의 검증 정보는 강하지만 최종 제품과 중간 증거를 분리해야 함 |

Wirelyre의 isolated-cell 검사는 채울 수 없는 세로 격리 공간을 제거한다.
imbalanced-split 검사는 영구적으로 분리된 빈 영역의 넓이가 tetromino 넓이 4로
나누어떨어지지 않는 경우를 제거한다. 이 수학적 조건은 참고할 수 있지만 원본
코드는 GPL-3.0-or-later이므로 복사하거나 vendoring하지 않는다. Clearra에서는
독립적으로 증명·구현하고 exhaustive differential과 no-false-negative KAT를
통과시켜야 한다.

## 4. 상태 수 실측

Wirelyre가 배포한
[`legal-boards.leb128`](https://wirelyre.github.io/tetra-tools/legal-boards.leb128)을
저장하지 않고 스트리밍 해석했다. 첫 unsigned LEB128은 전체 상태 수이고, 이후
값은 정렬된 이전 board와의 양의 차이다. 해석한 층별 상태 수는 다음과 같다.

| 피스 층 | Wirelyre 최종 `L_k` |
| ---: | ---: |
| 0 | 1 |
| 1 | 162 |
| 2 | 10,191 |
| 3 | 273,496 |
| 4 | 2,557,939 |
| 5 | 6,834,451 |
| 6 | 4,803,362 |
| 7 | 756,596 |
| 8 | 19,427 |
| 9 | 100 |
| 10 | 1 |
| **합계** | **15,255,726** |

Clearra SRS+ 역방향 실행은 `--max-new-steps 1`로 layer 6까지만 진행했다. 완성된
파일의 header와 실제 길이로 확인한 값은 다음과 같다.

| 피스 층 | Clearra 현재 `R_k` | Wirelyre 최종 `L_k` | 상태 수 비율 |
| ---: | ---: | ---: | ---: |
| 10 | 1 | 1 | 1.00x |
| 9 | 100 | 100 | 1.00x |
| 8 | 24,754 | 19,427 | 1.27x |
| 7 | 2,022,317 | 756,596 | 2.67x |
| 6 | **52,918,372** | **4,803,362** | **11.02x** |

규칙이 동일하지 않으므로 이 비율을 membership parity로 사용해서는 안 된다.
하지만 Wirelyre가 더 넓은 두 physics의 placement 합집합을 사용하면서도 최종
layer 6이 훨씬 작다는 점은 `R_6`과 `F_6 ∩ R_6`의 차이가 주원인이라는 진단과
일치한다.

## 5. 파일 크기 실측과 기여도 분리

Wirelyre 배포 자산은 23,560,813 bytes이며, 15,255,726개 전체 상태에 대해 평균
약 1.5444 bytes/state다.

Clearra의 완료된 SRS+ 역방향 디렉터리는 다음 크기였다.

| 파일 | 상태 수 | 크기 |
| --- | ---: | ---: |
| `reverse-layer-10.bin` | 1 | 136 bytes |
| `reverse-layer-09.bin` | 100 | 928 bytes |
| `reverse-layer-08.bin` | 24,754 | 198,160 bytes |
| `reverse-layer-07.bin` | 2,022,317 | 16,178,664 bytes |
| `reverse-layer-06.bin` | 52,918,372 | 423,347,104 bytes |
| owner metadata | - | 113 bytes |
| **합계** | **54,965,544** | **439,725,105 bytes** |

`PC4DOM02`의 layer 파일 크기는 정확히 `128 + 8 × state_count`다. 따라서 layer 6
하나만 Wirelyre 전체 파일보다 약 17.97배 크다.

집합은 유지하고 직렬화만 Wirelyre 방식처럼 정렬 차분 unsigned LEB128로 바꾼
이론값도 계산했다.

| 대상 | 크기 | 상태당 평균 |
| --- | ---: | ---: |
| Clearra layer 6 고정 `u64` payload | 423,346,976 bytes | 8.0000 bytes |
| 같은 52,918,372개 상태의 delta ULEB128 | 76,072,904 bytes | 1.437552 bytes |
| Wirelyre 전체 최종 자산 | 23,560,813 bytes | 1.5444 bytes |

직렬화 변경만으로 layer 6은 약 5.57배 줄지만, 압축된 layer 6 하나가 Wirelyre의
모든 층을 합친 파일보다 여전히 약 3.23배 크다. 따라서 우선순위는 다음과 같다.

1. `R_k` 전체가 아니라 최종 `F_k ∩ R_k`만 제품화한다.
2. 생성 중간물과 배포 자산의 이름·catalog·수명 주기를 분리한다.
3. 최종 집합에 compact serialization을 적용한다.

## 6. 생성 중 임시 디스크 사용량

SRS+ layer 7에서 layer 6을 만들 때 관측된 값은 다음과 같다.

- 입력 target: 2,022,317개
- 고유 `(source, piece)` 후보: 262,089,503개
- exact ILC 검증 후 source: 52,918,372개

현재 pair run의 최소 payload는 `u64 source hash + u8 piece`, 즉 후보당 9 bytes다.
따라서 후보 payload만 다음 크기다.

```text
262,089,503 × 9 = 2,358,805,527 bytes
```

이는 약 2.36 GB 또는 2.20 GiB다. 실제 피크에는 run header, 계층 병합 중 동시에
존재하는 입력·출력 run, validation checkpoint 및 최종 layer가 더해진다. 성공한
실행은 spill을 정리했으므로 완료 후에는 439,725,105 bytes만 남았다.

## 7. Clearra 제품 경계

앞으로 파일 역할을 다음처럼 고정한다.

- `forward-reachable-layer-N`: 빈 필드에서 도달 가능한 `F_k` 생성 증거
- `reverse-filter-layer-N`: 독립 검증이나 경계 증명에 필요한 `R_k` 중간 증거
- `legal-layer-N`: 최종 `F_k ∩ R_k`; 제품에서 사용할 수 있는 유일한 negative filter
- compact legal-board bundle: 검증된 `legal-layer-*`만 담는 profile별 배포 자산

`reverse-filter-layer-*`는 다음에 사용하지 않는다.

- GitHub legal-board release asset
- Web/CLI lazy-download catalog
- `illegal` 판정의 단독 권위
- TB hit/miss/offline 상태의 대체 표현

legal-board는 PC4 Tablebase와 독립된 제품이다. TB graph, HF generation,
activation keyring, range cache 또는 fallback 상태를 공유하지 않는다.

## 8. 독립 재설계 기준

Clearra의 권장 clean-room 생성 순서는 다음과 같다.

1. 정확한 kick profile과 ILC로 `F_0 ... F_10`을 생성한다.
2. `L_10 = F_10 ∩ {full}`로 시작한다.
3. 각 `k = 9 ... 0`에서 `F_k` 안의 필드만 predecessor 후보로 허용한다.
4. 각 predecessor에서 `L_{k+1}`로 가는 정확한 ILC 전이가 존재하는지 검증한다.
5. 검증된 `L_k`만 profile별 compact bundle에 기록한다.
6. isolated-cell·분리 영역 mod-4 등의 조기 prune은 독립 증명과 exhaustive
   differential을 통과한 뒤 계산 최적화로만 넣는다.
7. SRS, SRS+, SRS-X, Jstris 180, no-kick를 별도로 생성·검증·활성화한다.

최종 Go 조건은 profile마다 다음을 모두 만족하는 것이다.

- complete forward generation
- complete backward completion restriction
- deterministic state count와 digest
- exact result parity와 no-false-negative KAT
- corrupt/truncated/profile-mismatch asset의 fail-open 처리
- 실제 탐색의 compile/load/memory 비용을 포함한 A/B 우위

이 조건을 만족하기 전에는 현재 reverse 산출물을 legal-board 제품 자산으로
게시하거나 활성화하지 않는다.
