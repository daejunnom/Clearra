#!/usr/bin/env python3
"""Extract reviewable SRS+ data without executing the TETR.IO client bundle.

The input is a separately downloaded https://tetr.io/js/tetrio.js. Output stays
under the caller-selected artifact directory; reviewed fixtures can then be
promoted to tests/fixtures/rules. No network or JavaScript execution is used.
"""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import re


TRANSITIONS = ("01", "12", "23", "30", "03", "32", "21", "10", "02", "13", "20", "31")
PIECES = "IOTSZJL"
TOKEN = re.compile(r'\s*("(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[A-Za-z_$][A-Za-z0-9_$]*|[0-9]+(?:\.[0-9]+)?|[{}\[\]:,!+\-])')


class DataLiteral:
    """Allow only literal data, including minifier booleans !0 and !1."""

    def __init__(self, source):
        self.tokens = []
        position = 0
        while position < len(source):
            if not source[position:].strip():
                break
            match = TOKEN.match(source, position)
            if match is None:
                raise ValueError(f"Unsupported JavaScript token at {position}")
            self.tokens.append(match.group(1))
            position = match.end()
        self.index = 0

    def pop(self, expected=None):
        value = self.tokens[self.index]
        self.index += 1
        if expected is not None and value != expected:
            raise ValueError(f"Expected {expected!r}, received {value!r}")
        return value

    def value(self):
        token = self.pop()
        if token == "{":
            result = {}
            while self.tokens[self.index] != "}":
                key = self.pop()
                if key.startswith(('"', "'")):
                    key = ast.literal_eval(key)
                if key in result:
                    raise ValueError(f"Duplicate literal key {key!r}")
                self.pop(":")
                result[key] = self.value()
                if self.tokens[self.index] != ",":
                    break
                self.pop(",")
            self.pop("}")
            return result
        if token == "[":
            result = []
            while self.tokens[self.index] != "]":
                result.append(self.value())
                if self.tokens[self.index] != ",":
                    break
                self.pop(",")
            self.pop("]")
            return result
        if token in ("-", "+", "!"):
            operand = self.value()
            if not isinstance(operand, (int, float, bool)):
                raise ValueError("Unary operators require a literal number")
            return not operand if token == "!" else -operand if token == "-" else operand
        if token.startswith(('"', "'")):
            return ast.literal_eval(token)
        if token in ("true", "false", "null"):
            return {"true": True, "false": False, "null": None}[token]
        if token[0].isdigit():
            return float(token) if "." in token else int(token)
        raise ValueError(f"Nonliteral value {token!r}")

    def parse(self):
        result = self.value()
        if self.index != len(self.tokens):
            raise ValueError("Trailing JavaScript after data literal")
        return result


def brace_span(source, start):
    depth = 0
    quote = None
    escaped = False
    for position in range(start, len(source)):
        char = source[position]
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
        elif char in ('"', "'", "`"):
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return start, position + 1
    raise ValueError("Unterminated object or method")


def keyed_object(source, key):
    start = source.index(key + ":{") + len(key) + 1
    span = brace_span(source, start)
    return DataLiteral(source[slice(*span)]).parse(), span


def source_span(source, span):
    return {
        "byte_start": len(source[:span[0]].encode("utf-8")),
        "byte_end_exclusive": len(source[:span[1]].encode("utf-8")),
        "sha256": hashlib.sha256(source[slice(*span)].encode("utf-8")).hexdigest(),
    }


def extract(source_bytes, retrieved_date, audited_utc):
    source = source_bytes.decode("utf-8")
    kicksets, kick_span = keyed_object(source, "kicksets")
    pieces, piece_span = keyed_object(source, "tetrominoes")
    profile = kicksets["SRS+"]
    # A future source with extra origin corrections requires a reviewed converter.
    assert profile["additional_offsets"] == {}
    assert profile["spawn_rotation"] == {}
    assert not profile.get("allow_o_kick", False)
    assert "center_column" not in profile
    assert pieces["o"]["disallow_kick"] is True
    assert "kickset_special" not in pieces["o"]
    assert pieces["i"]["kickset_special"] == "i"
    for piece in PIECES.lower():
        assert "kickset_override" not in pieces[piece]
        if piece != "i":
            assert "kickset_special" not in pieces[piece]
        if piece != "o":
            assert not pieces[piece].get("disallow_kick", False)
    method_start = source.index("ComputeKick(e,t,n,s,i){")
    method_span = brace_span(source, source.index("{", method_start))
    method = source[slice(*method_span)]
    # Fail closed if the source no longer has the audited origin-before-kicks flow.
    assert method.index("IsLegalAtPos") < method.index("if(!g)return") < method.index("f.entries()")
    assert "return{x:_,y:m,r:i,kick:void 0}" in method
    assert "return{x:p,y:d,r:i,kick:{index:s,kx:a,ky:r}}" in method

    normalized = {}
    for piece in PIECES:
        for transition in TRANSITIONS:
            attempts = [[0, 0]]
            if piece != "O":
                family = "i_kicks" if piece == "I" else "kicks"
                attempts += profile[family][transition]
            assert all(len(pair) == 2 and all(type(v) is int for v in pair) for pair in attempts)
            normalized[f"{piece}:{transition}"] = [[dx, -dy] for dx, dy in attempts]
    assert len(normalized) == 84
    normalized_bytes = json.dumps(normalized, sort_keys=True, separators=(",", ":")).encode()
    return {
        "schema": "clearra.tetrio-srs-plus-standard-tetromino-kicks.v1",
        "authority": "https://tetr.io/js/tetrio.js",
        "retrieved_date": retrieved_date,
        "audited_utc": audited_utc,
        "source_sha256": hashlib.sha256(source_bytes).hexdigest(),
        "source_bytes": len(source_bytes),
        "source_axis": {"x": "right-positive", "y": "down-positive"},
        "clearra_axis": {"x": "right-positive", "y": "up-positive"},
        "implicit_origin_attempt": True,
        "first_success_order_preserved": True,
        "expected_standard_transition_count": 84,
        "expected_max_sequence_length_with_origin": 6,
        "standard_o": {"source_piece_id": "o", "disallow_kick": True, "origin_rotation_allowed": True, "uses_oo_kicks": False},
        "additional_offsets": profile["additional_offsets"],
        "spawn_rotation": profile["spawn_rotation"],
        "families": {"jlstz": {k: profile["kicks"][k] for k in TRANSITIONS}, "i": {k: profile["i_kicks"][k] for k in TRANSITIONS}},
        "standard_geometry_y_down": {piece: {"dx": pieces[piece.lower()]["matrix"]["dx"], "dy": pieces[piece.lower()]["matrix"]["dy"], "cells": [[[x, y] for x, y, *_ in shape] for shape in pieces[piece.lower()]["matrix"]["data"]]} for piece in PIECES},
        "transitions_y_up": normalized,
        "transitions_sha256": hashlib.sha256(normalized_bytes).hexdigest(),
        "source_spans": {"kicksets": source_span(source, kick_span), "tetrominoes": source_span(source, piece_span), "compute_kick_body": source_span(source, method_span)},
        "origin_guard_excerpt": "if(this.self.bm.IsLegalAtPos(e,_,m,i))return{x:_,y:m,r:i,kick:void 0};if(!g)return;",
    }


def compact_json(value, depth=0):
    """Keep numeric kick/geometry arrays on one line for reviewable diffs."""
    if not isinstance(value, dict):
        return json.dumps(value, ensure_ascii=False)
    if not value:
        return "{}"
    indent = "  " * depth
    return "{\n" + ",\n".join(indent + "  " + json.dumps(k) + ": " + compact_json(v, depth + 1) for k, v in value.items()) + "\n" + indent + "}"


def c_fixture(fixture):
    lines = [
        "/* Generated from the reviewed TETR.IO SRS+ numeric fixture.",
        " * Source SHA-256: " + fixture["source_sha256"],
        " * Regenerate with scripts/tools/extract-tetrio-srs-plus.py. */",
        "#ifndef CLEARRA_TETRIO_SRS_PLUS_FIXTURE_H",
        "#define CLEARRA_TETRIO_SRS_PLUS_FIXTURE_H",
        "static const ClearraCompactKickTransition TETRIO_SRS_PLUS_FIXTURE[84] = {",
    ]
    for key, offsets in fixture["transitions_y_up"].items():
        piece, transition = key.split(":")
        pairs = ", ".join("{" + str(dx) + ", " + str(dy) + "}" for dx, dy in offsets)
        lines.append("    {CLR_PIECE_" + piece + ", " + transition[0] + ", " + transition[1] + ", {{" + pairs + "}, " + str(len(offsets)) + "}},")
    return "\n".join(lines + ["};", "#endif", ""])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--retrieved-date", required=True)
    parser.add_argument("--audited-utc", required=True)
    args = parser.parse_args()
    fixture = extract(args.input.read_bytes(), args.retrieved_date, args.audited_utc)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = args.output_dir / "tetrio_srs_plus_standard_tetromino_kicks.json"
    output.write_text(compact_json(fixture) + "\n", encoding="utf-8")
    header = args.output_dir / "tetrio_srs_plus_fixture.h"
    header.write_text(c_fixture(fixture), encoding="utf-8")
    print(json.dumps({"output": str(output), "source_sha256": fixture["source_sha256"], "transitions": len(fixture["transitions_y_up"]), "transitions_sha256": fixture["transitions_sha256"]}))


if __name__ == "__main__":
    main()
