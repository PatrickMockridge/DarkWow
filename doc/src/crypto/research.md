# Research

This project maintains (inherited from upstream DarkWow) a public resource of zero-knowledge and math research
in the [`script/research/`](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research) directory of
the repo.

It features simple sage implementations of zero-knowledge algorithms
and math primitives, including but not limited to:

* [ZK](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/zk)
    * [bootle16](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/bootle16.py)
      (precursor to sonic)
    * [sonic](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/sonic.sage)
    * [plonk](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/plonk.sage)
    * [halo1](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/halo1.sage) and
      [halo2](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/halo2.sage)
    * [curve trees](https://github.com/PatrickMockridge/DarkWow/blob/linear-master/script/research/zk/curve_tree.sage)
    * [FFT](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/zk/fft)
    * [groth16](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/zk/groth16)
    * [bulletproofs](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/zk/bltprf)
* [Poseidon hash](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/poseidon)
* [x3dh](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/x3dh)
  double ratchet algorithm used in signal.
* [Various EC math](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/ec)
  such as valuations, riemann-roch basis, hyperelliptic curves, divisor reduction.

## Post-Quantum Cryptography

* [PQXDH (Post-Quantum Extended Diffie-Hellman)](https://github.com/PatrickMockridge/DarkWow/tree/linear-master/script/research/pqxdh/)
  — Hybrid key agreement (Kyber-1024 + X25519 + Double Ratchet) following Signal's
  PQXDH specification. Research implementation for post-quantum note encryption.
  See [Post-Quantum Architecture](../arch/quantum-threat.md#retroactive-privacy-protection).

* [NIST PQC Standards](https://csrc.nist.gov/projects/post-quantum-cryptography)
  — FIPS 203 (ML-KEM / Kyber), FIPS 204 (ML-DSA / Dilithium), FIPS 205 (SLH-DSA /
  SPHINCS+). Relevant for P2P signature hardening.

* STARK Proving Systems — Post-quantum ZK from collision-resistant hash functions.
  DarkWow's post-Halo2 migration target. See
  [Post-Quantum Proving System Requirements](../arch/zk/post-quantum-proving-system.md)
  for the formal swap-out specification (18 functional requirements).

* Lattice-Based ZK Proofs — Zero-knowledge from LWE/SIS assumptions (no known
  quantum break). Longer-term research target for compact post-quantum ZK proofs.
  See [Quantum Threat Model](../arch/quantum-threat.md#upgrade-paths).
