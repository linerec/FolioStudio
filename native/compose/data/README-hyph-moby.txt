Moby supplemental dictionary

The generated data contains 158136 lowercase ASCII words in 674862 bytes.
Generate it with Python's standard library:

  python next/tools/gen-hyph-moby.py <original-mhyph.txt> native/compose/data/hyph-moby.bin

Add --check to verify that the checked-in bytes are reproduced exactly.
The generator documents the binary format and prints the source SHA-256.
The original source is not stored in this repository. See LICENSE-hyph-moby.txt
and THIRD_PARTY_NOTICES.txt for the source, license and hashes.

The dictionary-first trial improved stored word agreement from 74.4% to 86.4%
(US) and 73.6% to 93.2% (UK), but introduced 14 print regressions out of 3278
scenarios. Runtime integration, including its environment switch, was reverted.
See artifacts/hyphmoby-c2-20261023/STATUS.ko.md and candidate-integration.patch
in the same artifact directory for evidence and the rejected implementation.

The c3 integration preserves every existing pattern point and the UK adjustment.
It only adds Moby points when no pattern point has at least two letters on each
side; added points must also meet that limit. Paragraph-specific limits still
apply afterward. No word-specific exception list is used.
Set FOLIO_NO_HYPH_DICT=1 before starting the engine to disable supplementation.
The switch does not alter the original patterns or their explicit exceptions.
See artifacts/hyphmoby-c3-20261023/STATUS.ko.md for the word-disjoint training /
holdout split, PDF comparison table and full print regression results.
