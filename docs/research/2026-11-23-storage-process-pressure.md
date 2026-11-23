# 저장소·프로세스·tmpfs 압력 감사 — 2026-11-23

## 1. 최초 11:06 UTC 환경 감사의 판정

**최초 11:06:36 UTC 환경 감사에서는 Clearra 자료를 삭제하거나 보존 저장소로 옮겨야
할 저장 공간 부족이 관측되지 않았다.** 당시 Clearra 작업공간의 overlay에는
41,380,237,312 B가 남아 있었다. 사용률 86%의 `/tmp`는 별도 tmpfs였으며, 당시
사용량 4,479,864,832 B는 다음 세 범주로 설명됐다. 이후 실행 경계는 §8·§9,
11:54 UTC의 큰 환경 변화는 §10, 두 cell-space 후보의 실행과 새 커널 OOM
카운터 증거는 §11·§12에 별도로 기록했다.

- 다른 프로젝트인 Mosk의 runtime 디렉터리: **4,467,679,232 B**.
- 소유 프로젝트를 특정하지 않은 공용 Node compile cache: **12,115,968 B**.
- Clearra 이름의 작은 helper 파일 8개: **69,632 B**의 할당 공간.

Mosk와 공용 cache는 보존한다. Clearra build cache는 overlay에 있으므로 이를 삭제해
위 tmpfs 사용량을 회수할 수 없다. 불필요한 재컴파일을 만들지 않도록 기존 build
cache도 보존한다. `_local/artifacts`와 manager receipt는 실행 증거이며 일반 cache로
분류하지 않는다. **최초 환경 감사 자체에서는** 삭제·업로드·빌드·시험·탐색·CI 실행을
하지 않았다. 이후 root가 별도로 수행한 빌드·테스트·탐색의 완료 자료를 읽어 추가한
기록과 구분한다.

최초 표본의 높은 shared-memory 사용량은 당시 tmpfs 파일 할당량으로 거의 전부
설명할 수 있었다. 그렇다고 **호스트에 orphan 프로세스가 전혀 없다거나, 이전 SIGKILL의
원인이 확인됐다는 뜻은 아니다.** 보이는 PID namespace와 호스트 메모리·mount의
관측 범위가 다르며, 이전 종료 순간의 host OOM/발신자 증거는 여전히 없다.

## 2. 관측 범위와 원시 자료

기준 source commit은 `e01c33c1be16269bcfdc755cd54d56b13b1c9f99`다. 주요 filesystem,
memory, process 표본은 **2026-11-23 11:06:36 UTC**에 읽었다. directory allocation,
inode, fd 및 기존 로그 검사는 같은 감사에서 이어서 읽은 별도 표본이다. 서로 다른
호출을 한 시점의 원자적 snapshot으로 취급하지 않는다.

선택한 원시 관측값과 범위·실행하지 않은 작업은 다음 파일에 보존했다.

`_local/artifacts/environment-storage-process-audit-20261123T110636Z.json`

자료는 `df -B1 -T`, `df -i`, `du -x -B1`, 지정 경로의 file metadata,
`/proc/meminfo`, 제한된 `ps` 필드, 현재 보이는 PID의 fd symlink metadata,
기존 P7P4 log/receipt의 명시적 storage error 검색 결과다. 환경변수, 전체 command
line, 자격 증명 내용은 읽지 않았다. 다른 프로젝트는 경로별 크기 metadata만 확인했고
파일 내용이나 하위 작업 내용을 조사하지 않았다.

## 3. filesystem별 공간과 inode

| Mount | 종류 | 전체 B | 사용 B | 남은 B | 표시 사용률 |
| --- | --- | ---: | ---: | ---: | ---: |
| `/workspace` | overlay | 67,586,535,424 | 22,753,550,336 | 41,380,237,312 | 36% |
| `/tmp` | tmpfs | 5,225,709,568 | 4,479,864,832 | 745,844,736 | 86% |
| `/home/agent` | tmpfs | 5,225,709,568 | 878,829,568 | 4,346,880,000 | 17% |
| `/dev/shm` | tmpfs | 5,225,709,568 | 0 | 5,225,709,568 | 0% |

Clearra repository와 `build/cargo/default`는 `/workspace`에 있다. `/tmp`의 낮은
잔여 공간과 같은 filesystem으로 합쳐 해석하지 않는다. `df`의 전체·사용·가용 값은
각 필드 그대로 보존했으며, 단순한 전체−사용 값으로 가용 공간을 대체하지 않았다.

| Mount | 전체 inode | 사용 inode | 남은 inode | 표시 사용률 |
| --- | ---: | ---: | ---: | ---: |
| `/workspace` | 4,194,304 | 149,207 | 4,045,097 | 4% |
| `/tmp` | 1,275,808 | 10,389 | 1,265,419 | 1% |
| `/home/agent` | 1,275,808 | 3,860 | 1,271,948 | 1% |

최초 감사 표본에는 block 또는 inode 고갈이 없다. 이 표는 이전 benchmark가 종료된
순간의 여유 공간을 소급 측정한 것이 아니다.

### `/tmp`의 할당량

| 경로·범주 | 할당 B | 소유권 판단 |
| --- | ---: | --- |
| `/tmp/mosk_r115_runtime` | 4,189,286,400 | 다른 프로젝트, 보존 |
| `/tmp/mosk_r116_runtime` | 69,066,752 | 다른 프로젝트, 보존 |
| `/tmp/mosk_r117_runtime` | 71,819,264 | 다른 프로젝트, 보존 |
| `/tmp/mosk_r118_runtime` | 70,242,304 | 다른 프로젝트, 보존 |
| `/tmp/mosk_r119_runtime` | 34,050,048 | 다른 프로젝트, 보존 |
| `/tmp/mosk_r120_runtime` | 33,214,464 | 다른 프로젝트, 보존 |
| `/tmp/node-compile-cache` | 12,115,968 | 공용 cache, 보존 |
| Clearra helper 파일 8개 | 69,632 | 소규모 일반 파일, 보존 |
| 기타 표시된 runtime 경로 | 0 | 환경·다른 프로젝트 경로, 보존 |
| **합계** | **4,479,864,832** | **`df` 사용량과 일치** |

Clearra helper는 `/tmp/clearra-` 뒤에 각각 `final-wording.mjs`,
`final-settings-docs.mjs`, `existing-test-contracts.mjs`, `release-runtime.mjs`,
`discord-surfaces.mjs`, `bot-deadlines.mjs`, `job-client.mjs`, `timeout-settings.mjs`가
붙는 8개다. 모두 일반 파일, UID 1000, hard-link count 1이며 논리적 내용 크기의
합은 51,294 B다. 내용은 읽거나 실행하지 않았다. 이름만으로 삭제 가능성을 확정하지
않았으며, 이 크기는 약 4.48 GB의 tmpfs 사용을 줄일 실질적 정리 대상도 아니다.

`du`에 표시된 경로들의 할당량이 `df` 사용량과 일치하므로, 현재 사용량을 설명하기
위해 별도의 대용량 열린 삭제 파일을 가정할 필요는 없다. 서로 다른 표본과 namespace의
한계가 있으므로 이를 모든 mount·모든 시점의 unlinked file 부재 증명으로 확대하지
않는다.

## 4. shared memory와 프로세스 관측

주요 `/proc/meminfo` 값은 다음과 같다. 원시 KiB 단위를 그대로 보존했다.

| 항목 | KiB |
| --- | ---: |
| MemTotal | 10,206,468 |
| MemAvailable | 4,202,480 |
| MemFree | 3,757,480 |
| Cached | 5,781,572 |
| Shmem | 5,233,124 |
| AnonPages | 53,132 |
| SwapTotal / SwapFree | 0 / 0 |

`Shmem`은 5,358,718,976 B다. `/tmp`와 `/home/agent`의 사용량 합은
5,358,694,400 B로, 두 값의 차이는 **24,576 B**다. 이 근접한 값은 현재 높은 shared
memory를 tmpfs 할당에서 먼저 설명해야 한다는 근거다. 각 shared page의 소유자를
식별한 결과도 아니며, `Cached`와 `Shmem`을 별도 사용량처럼 더하거나 프로세스 RSS와
단순 합산한 값도 아니다.

현재 tool namespace의 `ps`에는 PID 1 `codex`와 관측 명령인 PID 2 `ps`만 보였다.
RSS는 각각 5,972 KiB와 2,476 KiB였다. 이어서 보이는 PID 1/2의 fd metadata를
읽었을 때 각 3개 fd가 있었고, 열린 삭제 파일은 둘 다 0개였다.

| 질문 | 이번 근거로 판단 가능한 범위 |
| --- | --- |
| 현재 보이는 Clearra solver가 남아 있는가? | 표본의 PID 목록에 없다. |
| 호스트 전체에 orphan이 없는가? | 판단할 수 없다. namespace 밖 PID는 관측하지 못했다. |
| 큰 tmpfs 사용량이 이름 없는 프로세스에만 달려 있는가? | 현재 할당량은 이름이 있는 경로별 파일 사용으로 설명된다. |
| 파일을 쓰던 프로세스가 지금도 살아 있는가? | 크기 metadata만으로는 판정하지 않는다. 특히 Mosk의 내부 내용·프로세스 상태를 추적하지 않았다. |
| Clearra cache를 지우면 현재 Mosk tmpfs 사용이 줄어드는가? | 다른 filesystem의 Clearra build 삭제는 해당 경로의 할당을 회수하지 않는다. |

현재 `/proc/self/cgroup`은 `0::/`였지만 `/sys`와 cgroup filesystem이 노출되지
않았다. tracing/debugfs, journal 및 audit 디렉터리도 없었다. 관측 프로세스의 effective
capability는 0, NoNewPrivs는 1, Seccomp는 2였다. 이미 불가능함을 확인한 dmesg,
ptrace, perf 실험을 반복하지 않았다. 이 제한을 host에 resource limit이나 watchdog가
없다는 증거로 바꾸지 않는다.

## 5. Clearra 파일의 보존 판단

| 경로 | 할당 B | 위치·분류 | 현재 조치 |
| --- | ---: | --- | --- |
| `build/` 전체 | 8,489,852,928 | overlay, build 및 도구 출력 | 보존 |
| `build/cargo/default` | 8,179,195,904 | overlay, 활성 Cargo cache·출력·timings | 보존 |
| `build/cargo/recovery` | 66,568,192 | overlay, 복구 build 출력 | 보존 |
| `build/tools/node` | 244,076,544 | overlay, 지정 도구 | 보존 |
| `_local/artifacts` | 28,807,168 | overlay, 실행·계측 증거 | 보존 |
| `_local/tmp` | 32,768 | overlay, Clearra 지정 임시 경로 | 보존 |
| `/home/agent/.local/state/Clearra` | 2,842,624 | tmpfs, manager receipt | 보존 |

이 표는 부모·자식 디렉터리를 함께 표시하므로 합산하지 않는다. raw audit 파일과
이 문서를 저장하기 전의 값이다. Clearra의 home tmpfs state는 약 2.84 MB이며,
`/home/agent` 전체 878.83 MB를 Clearra가 쓴 것으로 귀속하지 않았다. 다른 home 경로는
범위를 넓혀 조사하지 않았다.

**현재 승인된 삭제 목록은 공집합이다.** 재생성 가능한 Cargo 출력이 있더라도 지금
여유가 충분해 삭제·재빌드 비용을 만들 이유가 없다. 또한 target 디렉터리에는 당시
실행의 Cargo timings처럼 재생성해도 같은 증거가 되지 않는 자료가 섞여 있다. 전체
target을 무조건 재생성 가능한 파일 묶음으로 취급하지 않는다.

고유 증거를 보존한 뒤 정리해야 할 조건이 생기면 실행 log/receipt, source·binary 지문,
timings와 검산 결과를 먼저 검증된 보존 경로에 보관해야 한다. 이번에는 그 조건이
충족되지 않아 Drive 업로드도 하지 않았다. 다른 프로젝트와 공용 cache의 정리 권한을
Clearra 작업의 권한으로 확대하지 않는다.

## 6. ENOSPC와 SIGKILL의 연결 여부

최근 세 P7P4 실행의 `.log`와 `.runtime.json`에서 `ENOSPC`, `no space left`,
disk full, `EDQUOT`, quota exceeded, storage exhausted를 확인했으며 일치하는 기록은
없었다. 대상 prefix는 다음과 같다.

- `_local/artifacts/full4-symbolic-ownership-20261123`
- `_local/artifacts/full4-symbolic-domain-transfer-20261123`
- `_local/artifacts/full4-symbolic-transfer-descriptor-20261123`

세 실행의 raw 종료는 signal 9 / code 137이고, manager의 wait 전후 종료 요청은
없었다. 자세한 audit는 [종료 소유권 보고서](2026-11-23-termination-ownership.md)에
있다. 현재 block·inode 여유, 파일별 tmpfs 합계, storage error 문자열 부재는
**Clearra의 디스크 고갈을 확정할 증거가 현재 없다는 결론**을 지지한다. 로그에
없다는 것만으로 다른 namespace·시점에서 storage 오류가 없었다고 증명하지 않는다.

tmpfs의 큰 사용량은 RAM 여유를 조사할 구체적인 환경 정보다. 그러나 현재 표본만으로
그 파일들이 각 이전 SIGKILL 시점에도 같은 양을 점유했다거나 커널 OOM·외부 종료
발신자가 확인됐다고 결론낼 수 없다. 다른 프로젝트 파일을 지우고 결과가 달라지는지
실험하는 방법도 이번 작업 범위에 포함되지 않는다.

## 7. 다음 측정에서 남길 진단

현재 자료의 가장 큰 공백은 각 실행 시작·종료 경계의 filesystem 및 memory snapshot이
없다는 점이다. 다음 실행에는 비교 하네스 밖에서 다음을 같은 실행 ID와 함께 기록하는
것이 유용하다.

1. 작업공간, 실제 `TMPDIR`, `/tmp`, `/home/agent`의 filesystem 종류·가용 bytes·inode.
2. MemAvailable·Shmem·SwapTotal 및 기존 kernel/표본 memory peak의 구분.
3. 기존 manager signal audit의 wait 전후 순서와 완료된 receipt·출력 파일의 존재.
4. start/end 표본의 시간 및 source tree·binary 지문. 샘플이 atomic하지 않다는 범위.

하나의 실행 직전에 memory와 disk 상태를 읽고 종료 직후 대조하면, 지금처럼 이후의
표본만으로 과거를 추론하는 범위를 줄일 수 있다. solver hot path에 filesystem 조회나
프로세스 전수 조사를 추가할 필요는 없다. 새로운 kernel 접근 권한을 얻거나 접근 불가
probe를 반복하는 계획도 아니다.

향후 빌드의 임시 파일은 허용된 overlay 경로인 `_local/tmp`를 `TMPDIR`로 명시하는
계획이다. Cargo의 `build/cargo/default`도 이미 overlay에 있다. 이 설정은 다른
프로젝트의 tmpfs 파일을 지우지 않으며 worker 수·메모리 계약·검색 제한을 줄이는
변경이 아니다. 이전 SIGKILL이 tmp 파일 생성에서 발생했다는 증거나 수정 완료의
주장과 구분한다. 실제 다음 실행의 시작·종료 값은 해당 prefix 자료가 준비된 뒤
별도로 추가한다.

## 8. Ready-domain/semijoin 후보 실행의 시작·종료 경계

`_local/artifacts/full4-symbolic-ready-dependencies-20261123` 실행에는
`.before.storage.json`과 `.after.storage.json`이 생겼다. 각각 **11:23:14 UTC**와
**11:23:33 UTC**의 별도 경계 표본이다. 시작 전·종료 후 모든 df/ps 호출이 성공했고
stderr는 비어 있었다. 이 실행에서는 허용된 `_local/tmp`를 임시 경로로 명시했으며,
작업공간과 해당 임시 경로가 모두 overlay인 것을 두 표본에서 확인했다.

### 저장 공간과 inode

| Mount | 시작 가용 B | 종료 가용 B | 시작 남은 inode | 종료 남은 inode |
| --- | ---: | ---: | ---: | ---: |
| `/workspace` (`.`와 `_local/tmp`) | 38,868,140,032 | 38,868,062,208 | 4,040,970 | 4,040,967 |
| `/tmp` | 745,844,736 | 745,844,736 | 1,265,419 | 1,265,419 |
| `/home/agent` | 4,346,814,464 | 4,346,753,024 | 1,271,945 | 1,271,944 |
| `/dev/shm` | 5,225,709,568 | 5,225,709,568 | 1,275,807 | 1,275,807 |

Filesystem별 전체 용량·inode 수는 앞 표와 같았다. overlay의 사용량은
25,265,647,616→25,265,725,440 B, `/tmp`는 4,479,864,832 B로 동일,
`/home/agent`는 878,895,104→878,956,544 B, `/dev/shm`은 0 B였다. 두 경계 모두
block이나 inode 고갈이 없다. 초기 감사의 약 41.38 GB와 이 실행의 약 38.87 GB
가용 공간은 서로 다른 시점의 값이며, 그 사이 build 출력을 포함한 환경 변화를
같은 snapshot으로 섞지 않는다.

### 메모리

| 항목 | 시작 B | 종료 B |
| --- | ---: | ---: |
| MemTotal | 10,451,423,232 | 10,451,423,232 |
| MemAvailable | 4,298,387,456 | 3,938,717,696 |
| MemFree | 2,975,866,880 | 3,963,777,024 |
| Cached | 6,793,912,320 | 5,403,398,144 |
| Shmem | 5,358,784,512 | 5,358,788,608 |
| AnonPages | 42,536,960 | 43,307,008 |
| SwapTotal | 0 | 0 |
| SwapFree | 0 | 0 |

이 값도 순차 표본이며 종료 순간의 peak가 아니다. Cached와 Shmem을 독립 사용량으로
합산하거나 종료 뒤의 MemAvailable로 자식이 살아 있던 순간의 여유를 대체하지
않는다. 경계 ps에는 codex와 snapshot을 읽는 bash/collector/ps만 보였다. 이것을
다른 host PID의 부재나 모든 orphan의 부재 증명으로 취급하지 않는다.

### 이 실행에서 가능한 결론

실행 receipt는 `1795432994982663532-9`, compiled source tree는
`6cd017819a608e70990ba775eb6a89bf8caccc09`다. 프로세스는 **18.129초에 signal 9 /
code 137로 종료**됐고, wait 전후 manager 종료 요청은 없었다. 탐색·identity export·
global coverage·joint comparison은 완료되지 않았다. rank 2 완료 marker의 첫 관측은
10.011775475초였고 이후 rank 3 movement가 관측됐다. 이 시각은 1초 progress 표본의
관측 경계이며 정확한 단계 완료 시각이 아니다.

해당 `.log`와 `.runtime.json`에서도 ENOSPC/EDQUOT/disk-full 문자열은 발견되지
않았다. **이번 시작·종료 경계에서 저장소가 고갈됐다는 설명은 자료와 맞지 않는다.**
다만 두 경계만으로 실행 중 매 순간이나 다른 mount namespace의 상태까지 증명하지는
않는다. 디스크 부족·내부 종료 발신자·커널 OOM 중 어느 것도 확정하지 않는다.

원시 근거는 같은 prefix의 `.before.storage.json`, `.after.storage.json`,
`.summary.json`, `.runtime.json`, `.observation.json`, `.movement-observation.json`이다.
이 추가 기록은 기존 자료를 읽어 작성했으며, 자료를 만들기 위한 새 실행·정리·업로드는
수행하지 않았다.

## 9. Private Boolean GC 후보 실행의 시작·종료 경계

최종 GC 후보의 prefix는 `_local/artifacts/full4-symbolic-private-gc-20261123`, receipt는
`1795433777287611454-9`다. `.before.storage.json`은 **11:36:17 UTC**,
`.after.storage.json`은 **11:36:38 UTC**의 표본이다. 각 df/ps 조회는 성공했고 stderr는
비어 있었다. `.`와 명시적 Clearra 임시 경로 `_local/tmp`는 두 경계 모두 overlay다.
아래 값은 이전 절의 실행이나 최초 환경 감사와 섞지 않은 이 실행의 경계 관측이다.

### 저장 공간과 inode

| Mount | 시작 가용 B | 종료 가용 B | 시작 남은 inode | 종료 남은 inode |
| --- | ---: | ---: | ---: | ---: |
| `/workspace` (`.`와 `_local/tmp`) | 38,687,444,992 | 38,687,346,688 | 4,040,583 | 4,040,580 |
| `/tmp` | 745,844,736 | 745,844,736 | 1,265,419 | 1,265,419 |
| `/home/agent` | 4,346,695,680 | 4,346,601,472 | 1,271,941 | 1,271,940 |
| `/dev/shm` | 5,225,709,568 | 5,225,709,568 | 1,275,807 | 1,275,807 |

overlay 사용량은 25,446,342,656→25,446,440,960 B였다. `/tmp`는
4,479,864,832 B로 변하지 않았고, `/home/agent`는 879,013,888→879,108,096 B,
`/dev/shm`은 0 B였다. 전체 filesystem 용량과 inode 수는 앞 절과 동일하다.
**이 실행의 시작·종료 경계에서도 block·inode 고갈은 관측되지 않았다.** 두 경계
표본이 실행 중 모든 순간이나 다른 mount namespace의 상태를 증명하지는 않는다.

### 메모리와 프로세스 관측 범위

| 항목 | 시작 B | 종료 B |
| --- | ---: | ---: |
| MemTotal | 10,451,423,232 | 10,451,423,232 |
| MemAvailable | 4,308,262,912 | 3,947,782,144 |
| MemFree | 3,228,221,440 | 3,975,860,224 |
| Cached | 6,544,252,928 | 5,397,147,648 |
| Shmem | 5,358,903,296 | 5,358,907,392 |
| AnonPages | 42,516,480 | 43,057,152 |
| SwapTotal | 0 | 0 |
| SwapFree | 0 | 0 |

경계 ps에는 codex와 경계를 읽는 bash/collector/ps만 보였다. 이는 해당 namespace에서
읽힌 프로세스 목록이며 다른 host PID나 orphan의 부재 증거가 아니다. 위 메모리 값도
순차 snapshot이며 자식이 살아 있던 시점의 peak 또는 동시 여유량이 아니다.

별도의 manager process-group RSS 표본 peak는 **2,812,809,216 B**, 자식의 1초 표본에서
읽힌 최대 VmHWM은 **2,812,477,440 B**, 최대 RSS는 **2,803,404,800 B**였다. 서로
scope와 표본 주기가 다르므로 같은 peak 측정으로 취급하지 않는다. manager가 실행
중 마지막으로 읽은 physical available은 1,105,137,664 B로, 종료 후 경계의
MemAvailable 3,947,782,144 B와 구별해야 한다. 어느 값도 신호 발신자를 식별하지 않는다.

### 요청·source 정합성과 종료 기록

이 실행의 `.summary.json`에서 확인한 두 fingerprint는 앞의 ready-dependencies 실행과
이전 transfer-descriptor 실행의 기록과 각각 정확히 같다. 각 summary의 `compiled_request`
및 `source` JSON block을 별도로 추출해 대조한 결과도 두 비교 모두 차이가 없었다.

- request SHA-256: `e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924`
- source SHA-256: `989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56`

조건은 empty 4L/P7P4/Jstris180, 10 locks·11 draws, Hold 허용·초기 Hold 없음,
FullQueueOracle, CountUnique, original source 4,233,600개다. worker 9개와 모든 논리
프로세서 사용 정책, max_seconds=0 및 implicit deadline 없음도 유지됐다. GC 후보의
compiled source tree는 `fa0f6c84b4e821078e9cf0d594cfeb52023e2e7a`, 실행 binary SHA-256은
`2307a81a9d42bdd4a8e8680599875802cf8b20472ebae0b719b18ad4be3732ba`다. 같은 요청과
source를 사용했다는 사실은 서로 다른 구현의 완료 성능이나 결과 일치를 증명하지 않는다.

프로세스는 **21.211초에 raw signal 9 / code 137로 종료**됐다. rank 2 완료의 첫
progress 표본은 **9.00721956초**이며, 마지막 관측은 rank 3 movement다. rank 3과 전체
탐색·identity export·global coverage·joint comparison은 완료되지 않았다. 완료 marker의
첫 관측은 표본 경계이므로 정확한 내부 단계 종료 시각과 동일시하지 않는다.

Receipt의 `termination_audit.attempts=[]`, `unretained_attempts=0`이며, 종료 요청 stderr
event도 비어 있었다. `supervisor_termination_reason=null`, timeout=0, memory pressure
events=0·observed=false, automatic_retry=false다. 종료 후에는 `process_tree_stopped=true`,
`descendant_processes_at_exit=0`이 기록됐다. 이 자료에 manager의 신호 발신은 관측되지
않았지만, 커널 또는 다른 프로세스 중 누가 signal 9를 보냈는지는 판정할 수 없다.

직접 자식과 supervisor의 초기 `/proc` 기록에서 CPU·data·RSS·address-space 제한은
unlimited였지만, 이것은 호스트 전체의 별도 cgroup·watchdog 제한 부재를 뜻하지 않는다.
관측 가능한 finite cgroup limit과 OOM-kill delta는 여전히 null이다. 따라서 이 실행도
OOM을 확정하거나 배제하지 않으며, 내부 발신자 또는 저장 공간 고갈이 원인이라고
확정하지 않는다. 현재 관측으로 다른 프로젝트·공유 캐시·Clearra 증거를 삭제할 이유는
추가되지 않았다.

근거는 같은 prefix의 `.summary.json`, `.observation.json`, `.runtime.json`,
`.before.storage.json`, `.after.storage.json`이다. 이 절은 해당 완료 자료를 제한된 범위로
읽어 작성했으며, 추가 실행·정리·업로드·권한 probe를 하지 않았다.

## 10. Cell-space 테스트 시작 전 확인된 환경 변화

`_local/artifacts/symbolic-cell-boolean-tests-20261123.before.storage.json`의
**11:54:32 UTC** 표본에서 tmpfs 사용량과 Shmem이 크게 줄었다. 이 snapshot은
cell-space 구현의 **테스트 시작 경계**이며, 새 P7P4 탐색의 완료 결과가 아니다.
기록된 source commit은 `833d5893cd84679e3457a9f8310b7e966cd954e2`다.

### 이전 경계와의 대조

| 항목 | 직전 GC 종료 경계 11:36:38 UTC | 테스트 시작 경계 11:54:32 UTC |
| --- | ---: | ---: |
| `/tmp` 사용 B | 4,479,864,832 | 12,185,600 |
| `/tmp` 가용 B | 745,844,736 | 5,213,523,968 |
| `/tmp` 남은 inode | 1,265,419 | 1,275,733 |
| `/home/agent` 사용 B | 879,108,096 | 879,108,096 |
| `/dev/shm` 사용 B | 0 | 0 |
| Overlay 사용 B | 25,446,440,960 | 31,675,826,176 |
| Overlay 가용 B | 38,687,346,688 | 32,457,961,472 |
| Overlay 남은 inode | 4,040,580 | 4,029,261 |
| MemAvailable B | 3,947,782,144 | 8,790,360,064 |
| Shmem B | 5,358,907,392 | 891,318,272 |

새 표본의 MemTotal은 10,451,423,232 B로 같았고, MemFree는 2,883,690,496 B,
Cached는 6,807,400,448 B, AnonPages는 45,006,848 B, SwapTotal/SwapFree는 모두 0 B였다.
df/ps 조회는 성공했으며 stderr는 비어 있었다. `.`와 Clearra `_local/tmp`는 여전히
overlay다. 이 값들은 순차 표본이며 독립적인 사용량처럼 Cached와 Shmem을 더하지 않는다.

`/tmp` 사용량의 감소는 **4,467,679,232 B**로, 최초 감사에서 여섯 Mosk 디렉터리에
보고된 할당량의 합계와 정확히 같다. Overlay 사용량은 같은 두 표본 사이에
6,229,385,216 B 늘었다. 이 산술 관계만으로 어떤 파일을 누가 어디로 옮겼는지,
overlay 증가가 어느 프로젝트의 출력 때문인지 판정할 수 없다.

### 같은 경로의 현재 상태

**11:55:43 UTC** 부근에 `/tmp`의 immediate-directory 크기와 기존 여섯 경로의
`stat` metadata만 읽었다. `du -x -B1 --max-depth=1 /tmp`는
`/tmp/node-compile-cache` 12,115,968 B, `/tmp` 전체 12,185,600 B를 보고했다.
stockfish 및 codex runtime용 기존 디렉터리의 보고 사용량은 0 B였다. 이 조회는
다른 프로젝트의 파일 내용이나 command line을 읽지 않았다.

기존 `/tmp/mosk_r115_runtime`, `/tmp/mosk_r116_runtime`, `/tmp/mosk_r117_runtime`,
`/tmp/mosk_r118_runtime`, `/tmp/mosk_r119_runtime`, `/tmp/mosk_r120_runtime`은 현재
모두 **symbolic link**로 존재한다. 각 link의 UID는 1000, link count는 1,
link-text 길이는 69 B였다. 이 경로들이 사라졌다고 기술하지 않는다. Link의 대상이나
대상 내용을 읽거나 따라가지 않았으므로, 대상의 위치·유효성·내용 보존 여부까지
확인한 기록도 아니다. `du`는 link를 따라가 그 대상의 사용량을 `/tmp`에 합산하지 않는다.

이 Clearra 작업에서는 해당 프로젝트나 공유 캐시의 정리·이동·삭제·아카이브를 하지
않았다. 이번 확인 역시 읽기 전용이며, 실제 변경 주체와 변경 절차는 알 수 없다.
기존 보존 경계는 그대로 유지하며 link나 그 대상에 쓰기·삭제 작업을 하지 않는다.

### 성능·메모리 비교에 미치는 영향

직전 GC P7P4 실행의 시작 MemAvailable은 4,308,262,912 B였고, 이번 테스트 전에는
8,790,360,064 B였다. Shmem과 filesystem 배치도 바뀌었다. 따라서 이후 cell-space
실행이 더 오래 진행하거나 완료하더라도, 그 차이 전부를 알고리즘 개선으로 설명할
수 없다. 시작 조건·source tree·worker 정책·실제 진행 단계와 peak 측정 범위를 함께
비교해야 한다. RSS가 측정되더라도 실행이 도달한 단계가 다르면 같은 작업량의
메모리 개선으로 환산하지 않는다.

이 환경 변화는 과거 signal 9의 발신자를 새로 식별한 증거가 아니다. 과거의 OOM이나
외부 종료를 확정·배제하지 않으며, 이후 비교의 환경 조건이 달라졌다는 사실로 남긴다.
근거 snapshot은 위 `.before.storage.json`이며, immediate-directory 크기와 link 상태는
별도로 수행한 metadata 조회다. 다른 프로젝트 내용 조사나 환경 변경은 수행하지 않았다.

## 11. Cell-space 중간 후보 실행과 커널 OOM 카운터

다음 기록은 첫 cell-space **중간 후보**에 대한 것이다. 이후 구조 변경의 최종 결과와
섞지 않는다. Prefix는 `_local/artifacts/full4-symbolic-cell-occupancy-20261123`,
receipt는 `1795435254914677505-9`, compiled source tree는
`791cb54f5bded0c0224e7e8209f3acbe103a10e4`다. 실행 binary SHA-256은
`8a17d4256941b0bcc6331aafc5bbf5d276a68b449be3c6b8e9525abbda342acf`다.

### 실행과 저장소 경계

프로세스는 **444.039초에 raw signal 9 / code 137로 종료**됐다. 탐색·identity export·
global coverage·joint comparison은 미완료다. Rank 3의 첫 표본은 10.007195024초,
마지막 표본은 442.857241341초의 rank 3 / T / row 253 / `row_down`이었다.
Rank-completion marker를 관측한 map은 비어 있으므로 별도의 rank 완료 시각을 만들지 않는다.

`.before.storage.json`과 `.after.storage.json`은 각각 **12:00:54 UTC**, **12:08:19 UTC**다.
작업공간과 `_local/tmp`는 overlay였고 df/ps 조회는 성공했으며 stderr는 비어 있었다.

| Mount | 시작 가용 B | 종료 가용 B | 시작 남은 inode | 종료 남은 inode |
| --- | ---: | ---: | ---: | ---: |
| `/workspace` (`.`와 `_local/tmp`) | 32,498,880,512 | 32,475,529,216 | 4,029,130 | 4,029,023 |
| `/tmp` | 5,213,523,968 | 5,213,523,968 | 1,275,733 | 1,275,733 |
| `/home/agent` | 4,346,507,264 | 4,344,619,008 | 1,271,935 | 1,271,934 |
| `/dev/shm` | 5,225,709,568 | 5,225,709,568 | 1,275,807 | 1,275,807 |

Overlay 사용량은 31,634,907,136→31,658,258,432 B, `/tmp`는 두 경계 모두
12,185,600 B였다. 시작·종료의 block·inode 고갈은 없었다. 이 결론은 해당 두 경계의
것이며 실행 중 모든 시각이나 다른 namespace의 filesystem을 소급 증명하지 않는다.

### 메모리와 supervisor 관측

| 항목 | 시작 B | 종료 B |
| --- | ---: | ---: |
| MemAvailable | 8,765,673,472 | 8,818,610,176 |
| MemFree | 4,289,490,944 | 8,839,860,224 |
| Cached | 5,384,822,784 | 937,377,792 |
| Shmem | 891,412,480 | 893,284,352 |
| AnonPages | 42,299,392 | 42,876,928 |

MemTotal은 10,451,423,232 B, SwapTotal/SwapFree는 0 B로 같았다. Manager의 process-group
RSS 표본 peak는 **7,336,996,864 B**였고, 자식 1초 표본에서 읽은 최대 RSS와 VmHWM은
모두 **7,334,789,120 B**였다. Manager가 마지막으로 읽은 physical available은
1,479,692,288 B다. 종료 후 MemAvailable은 자식 생존 중의 여유나 최저값을 대체하지 않는다.

Manager의 `termination_audit.attempts=[]`, `unretained_attempts=0`이며 종료 요청 stderr
event도 없었다. `supervisor_termination_reason=null`, timeout=0, memory pressure
events=0·observed=false, automatic_retry=false다. 종료 뒤 process tree는 stopped,
descendant 수는 0으로 기록됐다. 관측 가능한 cgroup의 finite memory limit과 OOM-kill
delta는 여전히 null이며, 이를 커널 전체에 OOM 사건이 없었다는 뜻으로 읽으면 안 된다.

### 이번에 추가된 커널 전역 증거

같은 prefix의 `.vmstat-during.txt`에는 `/proc/vmstat`의 다음 값이 보존돼 있다.

| UTC | `oom_kill` |
| --- | ---: |
| 12:05:00 | 12 |
| 12:05:25 | 12 |
| 12:07:08 | 12 |
| 12:09:51, 실행 종료 뒤 | 13 |

**마지막 실행 중 표본과 종료 뒤 표본 사이에 커널 OOM-kill event counter가 1 증가했다.**
이는 이전의 RSS·signal 9만으로 한 추론과 구별되는 추가 커널 증거다. 확인한 upstream
Linux [`mm/oom_kill.c`](https://github.com/torvalds/linux/blob/master/mm/oom_kill.c)의
`__oom_kill_process`는 `count_vm_event(OOM_KILL)` 뒤 victim에 SIGKILL을 보낸다.
[`mm/vmstat.c`](https://github.com/torvalds/linux/blob/master/mm/vmstat.c)의 `all_vm_events`는
CPU별 event를 합산한다. Source 확인일은 2026-11-23이며 이 호스트의 정확한 kernel
revision을 새로 확인한 것은 아니다.

따라서 OOM 경로를 지지하는 근거는 더 강해졌지만, **이 카운터에는 victim PID가 없다.**
12:07:08→12:09:51 구간은 Clearra의 종료 시점 앞뒤를 포함하며 다른 프로세스의 사건도
포함할 수 있다. Counter가 1 늘었다고 Clearra PID의 원인이 확정되거나 정확히 한 PID만
종료됐다고 결론낼 수 없다. 시스템 OOM과 memcg OOM의 구분도 이 값만으로는 할 수 없다.
해당 PID를 특정하는 kernel victim 로그나 cgroup별 증거는 확보하지 못했다.

요청과 source는 직전 private-GC 실행의 `compiled_request`·`source` JSON block을
추출해 직접 비교했으며 차이가 없었다. Worker·한도 계약은 같아도 §10의 Shmem·가용 RAM
변화가 있으므로 서로 다른 후보의 21.211초와 444.039초를 동일 조건의 완료 성능으로
비교하지 않는다. 구현별 진행 단계와 메모리 구조는 별도 cell-space 보고서에 기록한다.

근거는 해당 prefix의 `.summary.json`, `.observation.json`, `.movement-observation.json`,
`.compaction-observation.json`, `.runtime.json`, `.before.storage.json`, `.after.storage.json`,
`.vmstat-during.txt`다. 이 추가 감사는 보존된 자료와 지정된 upstream source만 읽었다.
새 실행·정리·이동·삭제·업로드·권한 probe는 수행하지 않았다.

## 12. Cell-space piece-batch 후보의 저장소·종료 증거

이 절은 §11의 행별 기하 검사 후보 다음에 실행한 **piece 단위 배치 후보**를 다룬다.
Prefix는 `_local/artifacts/full4-symbolic-cell-piece-batch-20261123`, receipt는
`1795436508607349449-11`이다. Compiled source tree는
`54561f9a5bbb0b3067f09c9b2dd60f686550f246`, binary SHA-256은
`2866d1436de712460bc46a805a04c72cc4b540fe269da4da5f73840482f8de2e`다.
메타데이터는 `movement_variable_space=original-cells`,
`geometry_filter_scope=piece-union`으로 이번 계산 범위를 구분한다.

### 실행 범위와 요청 정합성

프로세스는 **285.243초에 raw signal 9 / code 137로 종료**됐다. 전체 탐색·identity
export·global coverage·joint comparison은 완료되지 않았다. Rank 3의 첫 표본은
7.011661830초이고, 마지막 표본은 **284.314960197초의 rank 3 / T / `piece_down`**이다.
배치 전체를 검사하는 단계이므로 이 표본의 row는 null이다. T의 movement 상태는
`consuming`, 다른 여섯 미노는 모두 `released`였다. Rank-completion marker map은
비어 있으므로 rank 3 첫 표본을 rank 2의 정확한 내부 완료 시각으로 바꾸어 쓰지 않는다.

Rust 집계 자료
`_local/artifacts/symbolic-cell-candidate-comparison-20261123.json`은 §11 후보와 이번
후보의 `compiled_request` 및 원래 공급 `source` JSON이 각각 같음을 확인한다.
Request fingerprint는
`e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924`,
source fingerprint는
`989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56`으로 같다.
이 source는 공급·관측 계약을 가리키며 **서로 다른 compiled source tree가 같다는
뜻이 아니다.** Empty 4L/P7P4/Jstris180, 10 locks·11 draws, Hold 허용, CountUnique,
원래 source 4,233,600개와 auto/all-logical-processors·worker 9개 정책을 유지했다.
Timeout과 `max_seconds`는 0이고, 한도를 바꾼 자동 재시도는 없었다. 두 미완료 실행의
종료 시간 차이를 완료 성능 또는 동등 작업량의 처리량 개선으로 해석하지 않는다.

### 저장 공간과 inode 경계

같은 prefix의 `.before.storage.json`과 `.after.storage.json`은 각각
**2026-11-23 12:21:48 UTC**, **12:26:33 UTC**에 기록됐다. 이는 순차적으로 읽은 실행
전후 경계이며, 프로세스 시작·신호 발생 순간의 원자적 snapshot이 아니다. df/ps 조회는
성공했고 해당 조회의 stderr는 비어 있었다.

| Mount | 시작 가용 B | 종료 가용 B | 시작 남은 inode | 종료 남은 inode |
| --- | ---: | ---: | ---: | ---: |
| `/workspace` (`.`와 `_local/tmp`) | 32,370,495,488 | 32,369,299,456 | 4,028,874 | 4,028,865 |
| `/tmp` | 5,213,523,968 | 5,213,523,968 | 1,275,733 | 1,275,733 |
| `/home/agent` | 4,344,582,144 | 4,343,365,632 | 1,271,932 | 1,271,931 |
| `/dev/shm` | 5,225,709,568 | 5,225,709,568 | 1,275,807 | 1,275,807 |

Overlay 사용량은 31,763,292,160→31,764,488,192 B로 1,196,032 B 증가했다.
별도 `/tmp` tmpfs의 사용량은 두 경계 모두 **12,185,600 B**로 같았다. 관측한 시작·종료
경계에는 block·inode 고갈이 없었으며, 저장 공간 고갈을 이번 SIGKILL의 원인으로 연결할
증거는 없다. 이는 실행 중 모든 시각이나 다른 mount namespace의 상태를 증명하지 않는다.
이 자료를 근거로 다른 프로젝트·공용 cache·Clearra 실행 증거를 지울 이유는 추가되지 않았다.

### 메모리와 manager의 관측 범위

| 항목 | 시작 경계 B | 종료 경계 B |
| --- | ---: | ---: |
| MemAvailable | 8,786,919,424 | 8,588,681,216 |
| MemFree | 6,358,597,632 | 8,610,160,640 |
| Cached | 3,421,638,656 | 938,606,592 |
| Shmem | 893,337,600 | 894,509,056 |
| AnonPages | 42,082,304 | 43,634,688 |

MemTotal은 10,451,423,232 B, SwapTotal/SwapFree는 0 B로 같았다. 별도 manager 시작
표본의 physical available은 8,789,020,672 B였다. 이는 위 시작 경계 값과 다른 시점의
표본이며, 한 시점의 수치로 합치지 않는다.

자식 1초 표본에서 읽은 최대 RSS와 VmHWM은 모두 **7,306,452,992 B**였다. Manager의
process-group RSS 표본 peak는 **6,443,675,648 B**였다. 측정 범위와 주기가 다르므로
둘을 같은 peak 측정으로 취급하거나 더하지 않는다. Manager의 마지막 physical
available **4,215,803,904 B**는 별도로 갱신된 마지막 표본으로, 신호 발생 순간의 여유
메모리가 아니다. 이 값과 종료 후 MemAvailable을 OOM의 반증 또는 종료 순간의 최저
여유량으로 사용하지 않는다. Kernel peak 계측값은 null이다.

`termination_audit.attempts=[]`, `unretained_attempts=0`이고 종료 요청 stderr event도
비어 있었다. `supervisor_termination_reason=null`, timeout=0, memory pressure
events=0·observed=false, automatic_retry=false다. Manager의 pressure check는 143회,
cooperative GC 요청·응답은 각각 0회였다. 종료 후 `process_tree_stopped=true`,
`descendant_processes_at_exit=0`이 기록됐다. 따라서 보존된 manager 기록에는 종료 신호
발신이 없으며, 관리한 트리의 남은 descendant도 기록되지 않았다. 경계 ps는 해당 PID
namespace의 codex와 collector용 bash/ps만 보여 주므로, 이를 host 전체의 orphan 부재로
확대하지 않는다. 관측 가능한 finite cgroup limit과 hierarchical OOM-kill delta는 null이다.

### 실행 전후 커널 OOM 카운터

같은 prefix의 `.vmstat-before.txt`와 `.vmstat-after.txt`에는 다음 값이 보존돼 있다.

| UTC | `/proc/vmstat`의 `oom_kill` |
| --- | ---: |
| 12:21:48, 실행 전 경계 | 13 |
| 12:26:33, 실행 종료 뒤 경계 | 14 |

**실행을 둘러싼 두 경계 사이에 커널 OOM-kill event counter가 다시 1 증가했다.**
§11에서 확인한 counter의 의미와 함께 보면 이번 signal 9에도 OOM 경로를 지지하는
추가 증거다. 그러나 이 자료에는 victim PID가 없고, 같은 구간의 다른 프로세스 사건을
배제할 수 없다. Clearra PID가 OOM victim이었다고 확정하거나, 종료된 PID가 정확히
하나라고 결론내리지 않는다. 이 전역 counter만으로 시스템 OOM과 memcg OOM도 구별할
수 없다. 특정 PID의 kernel victim 로그나 해당 cgroup의 귀속 증거는 새로 확보하지 않았다.

이번 절의 근거는 prefix별 `.summary.json`, `.observation.json`,
`.movement-observation.json`, `.runtime.json`, `.before.storage.json`,
`.after.storage.json`, `.vmstat-before.txt`, `.vmstat-after.txt` 및 위 Rust 집계 파일이다.
알고리즘 단계·표현 크기 해석은
[`symbolic-cell-occupancy-2026-11-23.md`](symbolic-cell-occupancy-2026-11-23.md)에 분리했다.
이 추가 감사는 이미 보존된 자료만 읽었으며, 새 실행·probe·정리·이동·삭제·업로드를
수행하지 않았다. 다른 프로젝트와 공용 cache는 변경하지 않았다.

## 13. 2026-11-24 사용자 확인과 새 비교 환경

사용자가 이번 대화에서 **Mosk 자료 정리를 직접 요청했다고 확인했다.** §10의
"실제 변경 주체와 변경 절차는 알 수 없다"는 문구는 당시 filesystem metadata만으로
판단한 관측 범위다. 이제 정리가 사용자 요청에 따른 작업이었다는 맥락을 추가한다.
이 확인만으로 구체적인 이동 명령, 파일별 보존 결과 또는 변경 시각을 새로 검증한
것은 아니다. Clearra 작업에서는 Mosk 자료를 정리하거나 링크를 따라 읽지 않았다.

후속 작업은 scratch가 초기화된 환경에서 원격 feature commit
`671584db1d1c5b023ee1d82fbbf72421654c4d0e`를 복구하여 진행했다. 이전 ignored raw
자료와 build cache는 새 checkout에 없으며, Git에 보존된 검토 요약을 과거 증거로
사용한다. 새 legacy/current P7P4는 같은 복구 바이너리로 순차 실행하고, 실행 전후
filesystem·메모리 경계와 별도의 커널 종료 시 최대 RSS를 남긴다. 과거 환경의 실패
시간을 이번 실행의 성능 기준으로 섞지 않는다.

새 비교와 후속 변경의 완료 여부·메모리 범위는
[`p7p4-comparison-and-memory-2026-11-24.md`](p7p4-comparison-and-memory-2026-11-24.md)에
기록한다. 이번 실행에서 공간 정리나 Google Drive 보존이 필요해진 것으로 가정하지
않으며, 다른 프로젝트와 공용 cache의 보존 경계를 계속 지킨다.
