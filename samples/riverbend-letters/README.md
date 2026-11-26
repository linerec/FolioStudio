# 물굽이 서간 · Riverbend Letters

가상의 강변 도시 가람솔에서 펴낸 한인 생활 월간지다. 2026년 11월 창간호에는 시장이 서는 하루, 두 작업실의 비용 장부, 귤 향이 밴 무 요리, 소리로 지도를 그리는 교실과 동네 사람들이 낸 작은 광고를 담았다.

Folio Studio의 도움말에서 **잡지 샘플 열기**를 고르면 문서 폴더에 편집용 사본이 생긴다. 저장소에서는 이 폴더를 통째로 복사하고 `november.folio`를 연다. 그림과 글꼴도 프로젝트와 함께 옮긴다.

The November issue follows a market day in the fictional river town of Garamsol. Open **Help → Open Magazine Sample** for an editable copy. The 20-page US Letter project includes linked illustrations, Korean type, native tables and print profiles.

## 제작과 배포

- 글과 SVG 그림은 이 호를 위해 새로 만들었다. 그림별 제작 기록은 [CREDITS.md](assets/november/CREDITS.md)에 있다.
- 나눔명조 Regular와 나눔고딕 Bold는 SIL Open Font License 1.1로 배포한다. 원본 글꼴과 사용 조건은 [fonts](fonts/)에 있다.
- 인쇄용 색 프로파일 출처와 배포 조건은 [PRINT-PROFILE.md](PRINT-PROFILE.md)에 있다.
- 저장소 루트에서 아래 명령을 실행하면 새 폴더에 같은 프로젝트를 만든다. `--engine`에는 빌드한 Folio 엔진 실행 파일을 지정한다.

```sh
python next/tools/samples/riverbend.py --engine next/engine/target/release/folio-engine --output rebuilt-issue --work sample-work
```
