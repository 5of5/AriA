"""Python-surface coverage for the readout/tokenizer decode path.

Before this, nothing under `python/tests` imported the built `aria` extension
module at all — `test_training.py` only exercises the pure-Python training
scripts. This file covers the notebook-side decode seam (`aria.latents` +
`aria.Readout` + `aria.Tokenizer`) that mirrors what `aria emit` and the WASM
`emitIds` already do, so a run can be decoded to tokens without leaving
Python.
"""

from __future__ import annotations

import aria


def _config() -> aria.Config:
    # N = 8 is sub-spec; the escape is what lets a config this small validate
    # (mirrors AriaConfig::test_config() on the Rust side).
    return aria.Config(n_modes=8, latent_dim=16, allow_sub_spec_dims=True, seed=7)


def test_latents_length_and_dim_match_the_run() -> None:
    zs = aria.latents(steps=20, config=_config())
    assert len(zs) == 20
    assert all(len(z) == 16 for z in zs)


def test_latents_are_deterministic_for_a_fixed_seed() -> None:
    a = aria.latents(steps=10, config=_config())
    b = aria.latents(steps=10, config=_config())
    assert a == b


def test_seeded_discrete_readout_decodes_every_latent() -> None:
    zs = aria.latents(steps=10, config=_config())
    readout = aria.Readout.seeded_discrete(dim=16, vocab_size=256, temperature=1.0, seed=3)
    assert readout.kind == "discrete"
    assert readout.dim == 16

    ids = [readout.decode_id(z) for z in zs]
    assert len(ids) == 10
    assert all(0 <= i < 256 for i in ids)

    # decode_id is deterministic and matches the logit argmax.
    for z, i in zip(zs, ids):
        logits = readout.logits(z)
        assert i == max(range(len(logits)), key=lambda v: logits[v])


def test_discrete_probs_are_a_simplex() -> None:
    zs = aria.latents(steps=1, config=_config())
    readout = aria.Readout.seeded_discrete(dim=16, vocab_size=256, temperature=1.0, seed=3)
    probs = readout.probs(zs[0])
    assert len(probs) == 256
    assert all(p >= 0.0 for p in probs)
    assert abs(sum(probs) - 1.0) < 1e-9


def test_continuous_readout_rejects_discrete_only_calls() -> None:
    zs = aria.latents(steps=1, config=_config())
    readout = aria.Readout.seeded_continuous(dim=16, d_a=4, seed=9)
    assert readout.kind == "continuous"
    a = readout.emit(zs[0])
    assert len(a) == 4

    try:
        readout.decode_id(zs[0])
        raised = False
    except ValueError:
        raised = True
    assert raised, "decode_id on a continuous readout must raise, not silently misbehave"


def test_readout_bytes_round_trip_is_bit_identical() -> None:
    zs = aria.latents(steps=1, config=_config())
    readout = aria.Readout.seeded_discrete(dim=16, vocab_size=256, temperature=1.0, seed=3)
    restored = aria.Readout.from_bytes(readout.to_bytes())
    assert restored.decode_id(zs[0]) == readout.decode_id(zs[0])
    assert restored.probs(zs[0]) == readout.probs(zs[0])


def test_bpe_tokenizer_trains_and_decodes_every_id_it_can_produce() -> None:
    corpus = b"hello aria hello world hello aria world" * 4
    tok = aria.Tokenizer.train(corpus=corpus, vocab_size=260)
    assert tok.vocab_size <= 260

    ids = tok.encode(corpus)
    assert all(tok.decode_one(i) is not None for i in ids)
    assert bytes(tok.decode(ids)) == corpus


def test_bytes_identity_tokenizer_is_a_no_op_merge_table() -> None:
    tok = aria.Tokenizer.bytes_identity()
    assert tok.vocab_size == 256
    data = b"\x00\x01\xff hello"
    assert bytes(tok.decode(tok.encode(data))) == data
