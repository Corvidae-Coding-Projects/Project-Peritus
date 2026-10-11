# Terminal-Bench adverse-trial ledger

This ledger names every trial in the 445-trial frozen campaign that has at least one adverse
dimension: official reward is not 1, native Peritus did not accept, or Harbor recorded an exception.
It contains 347 rows. The remaining 98 trials are exactly those with reward 1, native acceptance,
and no exception.

The short terminal label is an index, not a replacement for evidence. Exact native messages,
exception text, timestamps, token counts, paths, and retained verifier results remain in
`/home/doll/.local/state/peritus/benchmarks/terminalbench/reports/frozen-baseline-445.final.json`
and its referenced trial directories.

| Task | Trial | Reward | Native | Native kind | Short native terminal | Exception |
| --- | --- | ---: | --- | --- | --- | --- |
| `adaptive-rejection-sampler` | `adaptive-rejection-sampler__BiJxgXz` | 0.0 | rejected | provider | no usable response | — |
| `adaptive-rejection-sampler` | `adaptive-rejection-sampler__QTBndEK` | 0.0 | rejected | provider | no usable response | — |
| `adaptive-rejection-sampler` | `adaptive-rejection-sampler__qPX8oma` | 0.0 | missing | — | — | AgentTimeoutError |
| `adaptive-rejection-sampler` | `adaptive-rejection-sampler__rfXj4up` | 1.0 | missing | — | — | AgentTimeoutError |
| `bn-fit-modify` | `bn-fit-modify__DVYNBLt` | 1.0 | rejected | provider | no usable response | — |
| `bn-fit-modify` | `bn-fit-modify__HrjYNvh` | U | accepted | — | — | RuntimeError |
| `bn-fit-modify` | `bn-fit-modify__PDV8XSX` | 1.0 | rejected | provider | no usable response | — |
| `break-filter-js-from-html` | `break-filter-js-from-html__PULZzFa` | 0.0 | rejected | provider | no usable response | — |
| `break-filter-js-from-html` | `break-filter-js-from-html__SHG5AAN` | 0.0 | rejected | provider | no usable response | — |
| `break-filter-js-from-html` | `break-filter-js-from-html__T562hZo` | 0.0 | rejected | provider | no usable response | — |
| `break-filter-js-from-html` | `break-filter-js-from-html__WmJGwMT` | U | rejected | provider | no usable response | RuntimeError |
| `break-filter-js-from-html` | `break-filter-js-from-html__ecQ7bg9` | 0.0 | rejected | provider | authentication | — |
| `build-cython-ext` | `build-cython-ext__9Wd28Zr` | 1.0 | rejected | provider | no usable response | — |
| `build-cython-ext` | `build-cython-ext__c7MW39v` | 1.0 | rejected | provider | context limit | — |
| `build-cython-ext` | `build-cython-ext__kWuJx6R` | 1.0 | rejected | provider | context limit | — |
| `build-cython-ext` | `build-cython-ext__n7YKgSt` | 1.0 | rejected | provider | no usable response | — |
| `build-cython-ext` | `build-cython-ext__nfg7zen` | 1.0 | missing | — | — | AgentTimeoutError |
| `build-pmars` | `build-pmars__5XnJWFC` | 1.0 | rejected | provider | context limit | — |
| `build-pmars` | `build-pmars__7mDnmHY` | 1.0 | rejected | provider | no usable response | — |
| `build-pmars` | `build-pmars__92VRvtc` | 0.0 | rejected | provider | no usable response | — |
| `build-pmars` | `build-pmars__ZM8nxBi` | 1.0 | rejected | provider | no usable response | — |
| `build-pmars` | `build-pmars__ndcU5kV` | 1.0 | rejected | provider | context limit | — |
| `build-pov-ray` | `build-pov-ray__KnEY232` | 0.0 | rejected | provider | context limit | — |
| `build-pov-ray` | `build-pov-ray__ZbbBfkk` | 1.0 | rejected | provider | image capability | — |
| `build-pov-ray` | `build-pov-ray__fGziNjS` | 1.0 | rejected | provider | image capability | — |
| `build-pov-ray` | `build-pov-ray__tjp2fTH` | 1.0 | rejected | provider | image capability | — |
| `build-pov-ray` | `build-pov-ray__wusJDu3` | U | rejected | provider | image capability | RuntimeError |
| `caffe-cifar-10` | `caffe-cifar-10__2WxBnnt` | 0.0 | missing | — | — | AgentTimeoutError |
| `caffe-cifar-10` | `caffe-cifar-10__JhMWenk` | 0.0 | rejected | provider | context limit | AgentTimeoutError |
| `caffe-cifar-10` | `caffe-cifar-10__UhqxGoH` | 0.0 | missing | — | — | AgentTimeoutError |
| `caffe-cifar-10` | `caffe-cifar-10__VXCJ8b7` | 0.0 | missing | — | — | AgentTimeoutError |
| `caffe-cifar-10` | `caffe-cifar-10__qjsjeor` | 0.0 | missing | — | — | AgentTimeoutError |
| `cancel-async-tasks` | `cancel-async-tasks__5rUhw9b` | 0.0 | accepted | — | — | — |
| `cancel-async-tasks` | `cancel-async-tasks__9D3BryS` | 0.0 | rejected | provider | no usable response | — |
| `cancel-async-tasks` | `cancel-async-tasks__aPpRDvK` | 0.0 | rejected | provider | no usable response | — |
| `cancel-async-tasks` | `cancel-async-tasks__nHxaBpG` | U | missing | — | — | RuntimeError |
| `chess-best-move` | `chess-best-move__RVre9kc` | 1.0 | rejected | provider | image capability | — |
| `chess-best-move` | `chess-best-move__y8S5jD4` | 1.0 | rejected | provider | no usable response | — |
| `chess-best-move` | `chess-best-move__yHACjmH` | 0.0 | rejected | provider | image capability | — |
| `circuit-fibsqrt` | `circuit-fibsqrt__9o85CN9` | 1.0 | rejected | provider | no usable response | — |
| `circuit-fibsqrt` | `circuit-fibsqrt__Ajr5gxi` | U | rejected | provider | no usable response | RuntimeError |
| `circuit-fibsqrt` | `circuit-fibsqrt__QvNaXxc` | 1.0 | rejected | provider | no usable response | — |
| `cobol-modernization` | `cobol-modernization__3MqQqNd` | 1.0 | rejected | provider | no usable response | — |
| `cobol-modernization` | `cobol-modernization__MWsWZvU` | 1.0 | rejected | provider | no usable response | — |
| `cobol-modernization` | `cobol-modernization__XSrd27K` | 1.0 | missing | — | — | AgentTimeoutError |
| `cobol-modernization` | `cobol-modernization__Y7Eke2H` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `code-from-image` | `code-from-image__4Wr2ZHH` | 1.0 | rejected | provider | image capability | — |
| `code-from-image` | `code-from-image__ji55Mj5` | U | rejected | provider | image capability | RuntimeError |
| `code-from-image` | `code-from-image__nPzTxpZ` | 1.0 | rejected | provider | image capability | — |
| `compile-compcert` | `compile-compcert__3f3Zq6N` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `compile-compcert` | `compile-compcert__7yzxgWZ` | 1.0 | rejected | provider | no usable response | — |
| `compile-compcert` | `compile-compcert__Xn3cf6z` | 0.0 | rejected | provider | no usable response | — |
| `compile-compcert` | `compile-compcert__ZeyZoTu` | U | rejected | provider | no usable response | RuntimeError |
| `configure-git-webserver` | `configure-git-webserver__nhwGwpy` | 1.0 | rejected | provider | no usable response | — |
| `configure-git-webserver` | `configure-git-webserver__nnC6YDJ` | U | rejected | provider | no usable response | RuntimeError |
| `configure-git-webserver` | `configure-git-webserver__ufYRVNA` | 0.0 | rejected | provider | no usable response | — |
| `constraints-scheduling` | `constraints-scheduling__4ccV4Vy` | 1.0 | rejected | provider | no usable response | — |
| `constraints-scheduling` | `constraints-scheduling__DckXeXQ` | 1.0 | rejected | provider | no usable response | — |
| `constraints-scheduling` | `constraints-scheduling__HF9wLuo` | 1.0 | rejected | waiting_for_user | phantom required path | — |
| `count-dataset-tokens` | `count-dataset-tokens__EhsmZ8H` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `count-dataset-tokens` | `count-dataset-tokens__HLV8ssn` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `count-dataset-tokens` | `count-dataset-tokens__HcDa4N9` | 1.0 | rejected | provider | no usable response | — |
| `count-dataset-tokens` | `count-dataset-tokens__ywsQNXk` | 1.0 | rejected | provider | no usable response | — |
| `crack-7z-hash` | `crack-7z-hash__3AnLGF2` | 0.0 | rejected | repository | artifact inventory bound | — |
| `crack-7z-hash` | `crack-7z-hash__gqdRV7a` | U | missing | — | — | RuntimeError |
| `crack-7z-hash` | `crack-7z-hash__hEdiy9N` | U | missing | — | — | RuntimeError |
| `crack-7z-hash` | `crack-7z-hash__j38mdEC` | U | missing | — | — | RuntimeError |
| `crack-7z-hash` | `crack-7z-hash__uD4ZhrE` | 0.0 | rejected | repository | artifact inventory bound | — |
| `custom-memory-heap-crash` | `custom-memory-heap-crash__rMuT39V` | 1.0 | rejected | provider | no usable response | — |
| `custom-memory-heap-crash` | `custom-memory-heap-crash__uogd7Sn` | 1.0 | rejected | provider | no usable response | — |
| `db-wal-recovery` | `db-wal-recovery__5V7WfCk` | 1.0 | rejected | provider | no usable response | — |
| `db-wal-recovery` | `db-wal-recovery__xVTomeQ` | 1.0 | rejected | provider | no usable response | — |
| `distribution-search` | `distribution-search__LesNjuQ` | 1.0 | rejected | provider | no usable response | — |
| `distribution-search` | `distribution-search__uehKJcJ` | 1.0 | rejected | provider | no usable response | — |
| `distribution-search` | `distribution-search__y8AHuXJ` | U | rejected | provider | no usable response | RuntimeError |
| `dna-assembly` | `dna-assembly__S6TDP4F` | 0.0 | rejected | provider | no usable response | — |
| `dna-assembly` | `dna-assembly__cPQ9NcV` | U | rejected | provider | no usable response | RuntimeError |
| `dna-assembly` | `dna-assembly__cRMeSvP` | 0.0 | accepted | — | — | — |
| `dna-assembly` | `dna-assembly__ywMxhKi` | 0.0 | rejected | provider | no usable response | — |
| `dna-assembly` | `dna-assembly__zznVSkd` | 1.0 | missing | — | — | AgentTimeoutError |
| `dna-insert` | `dna-insert__KD4hLQo` | 0.0 | rejected | provider | no usable response | — |
| `dna-insert` | `dna-insert__fvdEyyi` | 0.0 | accepted | — | — | — |
| `dna-insert` | `dna-insert__pkjcUjX` | 0.0 | rejected | provider | no usable response | — |
| `dna-insert` | `dna-insert__th8Q58w` | 0.0 | rejected | provider | no usable response | — |
| `dna-insert` | `dna-insert__zDReN7j` | 0.0 | accepted | — | — | — |
| `extract-elf` | `extract-elf__Fxyp3G5` | 1.0 | rejected | provider | ambiguous acceptance | AgentTimeoutError |
| `extract-elf` | `extract-elf__UV9kQhp` | 1.0 | missing | — | — | AgentTimeoutError |
| `extract-elf` | `extract-elf__ZetN3kN` | 0.0 | accepted | — | — | — |
| `extract-elf` | `extract-elf__iH5QU33` | 1.0 | rejected | provider | no usable response | — |
| `extract-elf` | `extract-elf__u7xYtyS` | 1.0 | rejected | provider | no usable response | — |
| `extract-moves-from-video` | `extract-moves-from-video__99zmcix` | 0.0 | missing | — | — | AgentTimeoutError |
| `extract-moves-from-video` | `extract-moves-from-video__CVxEpH2` | U | missing | — | — | RuntimeError |
| `extract-moves-from-video` | `extract-moves-from-video__gLP7k2n` | 0.0 | missing | — | — | AgentTimeoutError |
| `extract-moves-from-video` | `extract-moves-from-video__t7VnGw4` | 0.0 | missing | — | — | AgentTimeoutError |
| `extract-moves-from-video` | `extract-moves-from-video__wf7tsAp` | 0.0 | rejected | provider | no usable response | — |
| `feal-differential-cryptanalysis` | `feal-differential-cryptanalysis__iou7Ebr` | 0.0 | rejected | provider | no usable response | — |
| `feal-differential-cryptanalysis` | `feal-differential-cryptanalysis__rhWYjRp` | U | rejected | provider | no usable response | RuntimeError |
| `feal-differential-cryptanalysis` | `feal-differential-cryptanalysis__v8aeNWi` | 0.0 | rejected | provider | no usable response | — |
| `feal-linear-cryptanalysis` | `feal-linear-cryptanalysis__9LZmKm8` | 0.0 | rejected | provider | no usable response | — |
| `feal-linear-cryptanalysis` | `feal-linear-cryptanalysis__GXTw6Kw` | 1.0 | rejected | provider | no usable response | — |
| `feal-linear-cryptanalysis` | `feal-linear-cryptanalysis__RgGdfgJ` | 0.0 | rejected | provider | no usable response | — |
| `feal-linear-cryptanalysis` | `feal-linear-cryptanalysis__ayu2ydK` | 0.0 | rejected | provider | authentication | — |
| `filter-js-from-html` | `filter-js-from-html__2hVYPSz` | 0.0 | rejected | provider | no usable response | — |
| `filter-js-from-html` | `filter-js-from-html__9HFGuSi` | U | accepted | — | — | VerifierTimeoutError |
| `filter-js-from-html` | `filter-js-from-html__JzmM47N` | 0.0 | rejected | provider | no usable response | — |
| `filter-js-from-html` | `filter-js-from-html__gfhnTf4` | 0.0 | accepted | — | — | — |
| `filter-js-from-html` | `filter-js-from-html__pvSq8Pe` | 0.0 | missing | — | — | AgentTimeoutError |
| `financial-document-processor` | `financial-document-processor__5jkyPMr` | 0.0 | rejected | provider | context limit | — |
| `financial-document-processor` | `financial-document-processor__P6xNjCg` | 0.0 | rejected | provider | no usable response | — |
| `financial-document-processor` | `financial-document-processor__YNpQM4k` | 0.0 | missing | — | — | AgentTimeoutError |
| `financial-document-processor` | `financial-document-processor__gEQUEoJ` | 0.0 | rejected | provider | context limit | — |
| `financial-document-processor` | `financial-document-processor__pfa6ZdQ` | 0.0 | missing | — | — | AgentTimeoutError |
| `fix-code-vulnerability` | `fix-code-vulnerability__4ctaR8r` | 0.0 | missing | — | — | AgentTimeoutError |
| `fix-code-vulnerability` | `fix-code-vulnerability__KWhgG7Y` | 0.0 | rejected | waiting_for_user | in-scope failure handoff | — |
| `fix-code-vulnerability` | `fix-code-vulnerability__TeqBwoz` | 1.0 | rejected | provider | no usable response | — |
| `fix-code-vulnerability` | `fix-code-vulnerability__Tio6xcR` | U | rejected | provider | no usable response | RuntimeError |
| `fix-code-vulnerability` | `fix-code-vulnerability__vhy73Re` | 1.0 | rejected | provider | no usable response | — |
| `fix-git` | `fix-git__Ly54JZ6` | 1.0 | rejected | provider | no usable response | — |
| `fix-git` | `fix-git__UAY2jZJ` | 1.0 | rejected | repository | HEAD changed | — |
| `fix-git` | `fix-git__XdEbrDJ` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `fix-git` | `fix-git__YdD2yHn` | 1.0 | rejected | repository | HEAD changed | — |
| `fix-git` | `fix-git__azjqcAo` | 1.0 | rejected | provider | no usable response | — |
| `fix-ocaml-gc` | `fix-ocaml-gc__SqoTV8A` | U | missing | — | — | RuntimeError |
| `fix-ocaml-gc` | `fix-ocaml-gc__e6bKtb7` | 0.0 | rejected | repository | artifact inventory bound | — |
| `fix-ocaml-gc` | `fix-ocaml-gc__hGB6s8d` | U | missing | — | — | RuntimeError |
| `fix-ocaml-gc` | `fix-ocaml-gc__qrz7jeu` | U | missing | — | — | RuntimeError |
| `fix-ocaml-gc` | `fix-ocaml-gc__ynRNnVL` | 0.0 | rejected | repository | artifact inventory bound | — |
| `gcode-to-text` | `gcode-to-text__2G2enxJ` | 0.0 | accepted | — | — | — |
| `gcode-to-text` | `gcode-to-text__YYWt3UV` | 0.0 | missing | — | — | AgentTimeoutError |
| `gcode-to-text` | `gcode-to-text__c3qsSoz` | 0.0 | missing | — | — | AgentTimeoutError |
| `gcode-to-text` | `gcode-to-text__e5ePRiB` | 0.0 | rejected | provider | no usable response | — |
| `git-leak-recovery` | `git-leak-recovery__JdXSmsu` | 1.0 | rejected | provider | no usable response | — |
| `git-leak-recovery` | `git-leak-recovery__hkFoz9h` | 1.0 | rejected | provider | no usable response | — |
| `git-multibranch` | `git-multibranch__EhEMNrD` | 1.0 | missing | — | — | AgentTimeoutError |
| `git-multibranch` | `git-multibranch__PA6uwxo` | 1.0 | rejected | provider | no usable response | — |
| `git-multibranch` | `git-multibranch__fqjwGzx` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `gpt2-codegolf` | `gpt2-codegolf__AG8DbWx` | 1.0 | missing | — | — | AgentTimeoutError |
| `gpt2-codegolf` | `gpt2-codegolf__CYifUve` | 0.0 | rejected | provider | context limit | — |
| `gpt2-codegolf` | `gpt2-codegolf__KdWL9cf` | 0.0 | rejected | provider | no usable response | AgentTimeoutError |
| `gpt2-codegolf` | `gpt2-codegolf__agkczXm` | 0.0 | missing | — | — | AgentTimeoutError |
| `gpt2-codegolf` | `gpt2-codegolf__ndEsFdm` | 0.0 | missing | — | — | AgentTimeoutError |
| `headless-terminal` | `headless-terminal__TzD6e7a` | 1.0 | rejected | provider | no usable response | — |
| `headless-terminal` | `headless-terminal__fMw82kx` | 1.0 | missing | — | — | AgentTimeoutError |
| `headless-terminal` | `headless-terminal__wkdbxC3` | 1.0 | rejected | provider | no usable response | — |
| `hf-model-inference` | `hf-model-inference__4yJMwBn` | 0.0 | rejected | provider | no usable response | — |
| `hf-model-inference` | `hf-model-inference__QxiS3Gy` | 1.0 | rejected | provider | no usable response | — |
| `hf-model-inference` | `hf-model-inference__kXUdBfc` | 1.0 | rejected | provider | no usable response | — |
| `install-windows-3-11` | `install-windows-3-11__26jCfud` | 0.0 | accepted | — | — | — |
| `install-windows-3-11` | `install-windows-3-11__4zmGsT3` | 0.0 | rejected | provider | no usable response | — |
| `install-windows-3-11` | `install-windows-3-11__DTcvtyK` | 0.0 | accepted | — | — | — |
| `install-windows-3-11` | `install-windows-3-11__Qj4xuLV` | 0.0 | accepted | — | — | — |
| `install-windows-3-11` | `install-windows-3-11__vXdV2Ln` | 0.0 | accepted | — | — | — |
| `kv-store-grpc` | `kv-store-grpc__63mHzFg` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `kv-store-grpc` | `kv-store-grpc__F67n746` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `kv-store-grpc` | `kv-store-grpc__r9sYjKe` | 0.0 | rejected | provider | no usable response | — |
| `large-scale-text-editing` | `large-scale-text-editing__AprDm2B` | 1.0 | rejected | provider | no usable response | — |
| `large-scale-text-editing` | `large-scale-text-editing__BoiBfsz` | 1.0 | rejected | provider | context limit | — |
| `large-scale-text-editing` | `large-scale-text-editing__LgDV62Y` | U | accepted | — | — | RuntimeError |
| `large-scale-text-editing` | `large-scale-text-editing__QXadNCY` | 1.0 | rejected | provider | no usable response | — |
| `large-scale-text-editing` | `large-scale-text-editing__pJYQxzn` | 1.0 | rejected | provider | context limit | — |
| `largest-eigenval` | `largest-eigenval__LCuzJte` | 0.0 | rejected | provider | no usable response | — |
| `largest-eigenval` | `largest-eigenval__TKbqK4H` | 1.0 | rejected | provider | no usable response | — |
| `largest-eigenval` | `largest-eigenval__mnKpTj8` | 1.0 | rejected | provider | no usable response | — |
| `largest-eigenval` | `largest-eigenval__owrfP9W` | 0.0 | accepted | — | — | — |
| `llm-inference-batching-scheduler` | `llm-inference-batching-scheduler__4u229n3` | 1.0 | rejected | provider | no usable response | — |
| `llm-inference-batching-scheduler` | `llm-inference-batching-scheduler__AgymEAt` | U | rejected | provider | no usable response | RuntimeError |
| `llm-inference-batching-scheduler` | `llm-inference-batching-scheduler__VmXQPrh` | 1.0 | rejected | provider | no usable response | — |
| `log-summary-date-ranges` | `log-summary-date-ranges__fvFKj7V` | 1.0 | rejected | provider | no usable response | — |
| `log-summary-date-ranges` | `log-summary-date-ranges__uY3w2WY` | 1.0 | rejected | provider | no usable response | — |
| `mailman` | `mailman__9Xxhvvu` | 1.0 | rejected | provider | no usable response | — |
| `mailman` | `mailman__YNhEKbG` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `mailman` | `mailman__ZpR6gRM` | 1.0 | rejected | provider | no usable response | — |
| `make-doom-for-mips` | `make-doom-for-mips__Hr7kpWw` | 0.0 | rejected | provider | no usable response | — |
| `make-doom-for-mips` | `make-doom-for-mips__PFTL7J9` | 0.0 | rejected | provider | context limit | — |
| `make-doom-for-mips` | `make-doom-for-mips__W3Si6em` | 0.0 | rejected | provider | context limit | — |
| `make-doom-for-mips` | `make-doom-for-mips__ck8ykWK` | 0.0 | rejected | provider | no usable response | — |
| `make-doom-for-mips` | `make-doom-for-mips__zEJyjRC` | U | rejected | provider | no usable response | RuntimeError |
| `make-mips-interpreter` | `make-mips-interpreter__RzvKfH2` | U | rejected | provider | no usable response | RuntimeError |
| `make-mips-interpreter` | `make-mips-interpreter__Z3Vjfh5` | 0.0 | rejected | provider | no usable response | — |
| `make-mips-interpreter` | `make-mips-interpreter__ispbSkU` | 1.0 | rejected | provider | no usable response | — |
| `make-mips-interpreter` | `make-mips-interpreter__rBJEZ4r` | 0.0 | rejected | provider | context limit | — |
| `mcmc-sampling-stan` | `mcmc-sampling-stan__JrPLENv` | 0.0 | rejected | provider | no usable response | — |
| `mcmc-sampling-stan` | `mcmc-sampling-stan__gYsh97i` | 0.0 | rejected | provider | no usable response | — |
| `mcmc-sampling-stan` | `mcmc-sampling-stan__nNAaDtz` | 0.0 | rejected | provider | context limit | — |
| `mcmc-sampling-stan` | `mcmc-sampling-stan__tRwa6ha` | U | rejected | provider | no usable response | RuntimeError |
| `mcmc-sampling-stan` | `mcmc-sampling-stan__yS7fTbw` | 0.0 | rejected | provider | context limit | — |
| `merge-diff-arc-agi-task` | `merge-diff-arc-agi-task__WtSsHiN` | 1.0 | missing | — | — | AgentTimeoutError |
| `merge-diff-arc-agi-task` | `merge-diff-arc-agi-task__fsoZd6p` | 1.0 | rejected | provider | no usable response | — |
| `merge-diff-arc-agi-task` | `merge-diff-arc-agi-task__gF9UUgd` | 1.0 | missing | — | — | AgentTimeoutError |
| `merge-diff-arc-agi-task` | `merge-diff-arc-agi-task__hRvqJrw` | 1.0 | missing | — | — | AgentTimeoutError |
| `merge-diff-arc-agi-task` | `merge-diff-arc-agi-task__vZyK6fA` | U | missing | — | — | RuntimeError |
| `model-extraction-relu-logits` | `model-extraction-relu-logits__FT8yCrj` | 1.0 | missing | — | — | AgentTimeoutError |
| `model-extraction-relu-logits` | `model-extraction-relu-logits__Q9XBVCw` | 1.0 | rejected | provider | no usable response | — |
| `model-extraction-relu-logits` | `model-extraction-relu-logits__ep6fVnF` | 0.0 | rejected | provider | no usable response | — |
| `model-extraction-relu-logits` | `model-extraction-relu-logits__w2mGFUx` | 0.0 | rejected | provider | no usable response | — |
| `modernize-scientific-stack` | `modernize-scientific-stack__D3QHyZC` | U | rejected | gate | unchanged fixer cycles | RuntimeError |
| `modernize-scientific-stack` | `modernize-scientific-stack__SFwzixP` | 1.0 | rejected | provider | no usable response | — |
| `modernize-scientific-stack` | `modernize-scientific-stack__epZMaUw` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `modernize-scientific-stack` | `modernize-scientific-stack__mxFVsds` | 1.0 | rejected | provider | no usable response | — |
| `modernize-scientific-stack` | `modernize-scientific-stack__xQzT43o` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `mteb-leaderboard` | `mteb-leaderboard__C9RPt7U` | 0.0 | rejected | provider | no usable response | — |
| `mteb-leaderboard` | `mteb-leaderboard__amJsaPs` | 0.0 | missing | — | — | AgentTimeoutError |
| `mteb-leaderboard` | `mteb-leaderboard__jXfVzpW` | 0.0 | rejected | provider | no usable response | — |
| `mteb-leaderboard` | `mteb-leaderboard__ngm4jT9` | 0.0 | accepted | — | — | — |
| `mteb-leaderboard` | `mteb-leaderboard__svjmNWD` | 0.0 | rejected | provider | context limit | — |
| `mteb-retrieve` | `mteb-retrieve__DKpVJtd` | 0.0 | accepted | — | — | — |
| `mteb-retrieve` | `mteb-retrieve__Sbf7buL` | 0.0 | rejected | provider | no usable response | — |
| `mteb-retrieve` | `mteb-retrieve__VHRKdAd` | 0.0 | accepted | — | — | — |
| `mteb-retrieve` | `mteb-retrieve__Z4Kmrrj` | 0.0 | rejected | provider | no usable response | — |
| `mteb-retrieve` | `mteb-retrieve__hxuvkaf` | U | accepted | — | — | RuntimeError |
| `multi-source-data-merger` | `multi-source-data-merger__EnoFUos` | 1.0 | rejected | provider | no usable response | — |
| `multi-source-data-merger` | `multi-source-data-merger__gcA3mNH` | U | missing | — | — | RuntimeError |
| `nginx-request-logging` | `nginx-request-logging__3q7fktm` | 1.0 | missing | — | — | AgentTimeoutError |
| `nginx-request-logging` | `nginx-request-logging__96C9xYf` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `nginx-request-logging` | `nginx-request-logging__SBrU2rp` | 1.0 | rejected | provider | no usable response | — |
| `openssl-selfsigned-cert` | `openssl-selfsigned-cert__SWZsjnd` | 1.0 | rejected | provider | no usable response | — |
| `openssl-selfsigned-cert` | `openssl-selfsigned-cert__dogUcYg` | 1.0 | rejected | provider | no usable response | — |
| `overfull-hbox` | `overfull-hbox__SSkmz36` | 0.0 | rejected | provider | no usable response | — |
| `overfull-hbox` | `overfull-hbox__STL7hyL` | 1.0 | rejected | provider | no usable response | — |
| `overfull-hbox` | `overfull-hbox__nmizTfd` | U | rejected | provider | no usable response | RuntimeError |
| `password-recovery` | `password-recovery__64eTs6y` | 1.0 | rejected | provider | no usable response | — |
| `password-recovery` | `password-recovery__MFBsNm9` | 0.0 | missing | — | — | AgentTimeoutError |
| `password-recovery` | `password-recovery__gXhPa8g` | U | rejected | provider | no usable response | RuntimeError |
| `password-recovery` | `password-recovery__h9XtQX4` | 1.0 | rejected | provider | no usable response | — |
| `path-tracing-reverse` | `path-tracing-reverse__6jL5tLQ` | 1.0 | missing | — | — | AgentTimeoutError |
| `path-tracing-reverse` | `path-tracing-reverse__924etyD` | 1.0 | rejected | provider | no usable response | — |
| `path-tracing-reverse` | `path-tracing-reverse__vZBxiH8` | 1.0 | missing | — | — | AgentTimeoutError |
| `path-tracing-reverse` | `path-tracing-reverse__vvfmFdo` | 0.0 | rejected | provider | no usable response | — |
| `path-tracing` | `path-tracing__JmyaeU9` | 0.0 | rejected | provider | no usable response | — |
| `path-tracing` | `path-tracing__aSGutji` | U | rejected | provider | no usable response | RuntimeError |
| `path-tracing` | `path-tracing__bbhas2Q` | 1.0 | rejected | provider | no usable response | — |
| `polyglot-c-py` | `polyglot-c-py__8gjH98c` | 0.0 | accepted | — | — | — |
| `polyglot-c-py` | `polyglot-c-py__8kNNAB6` | 0.0 | accepted | — | — | — |
| `polyglot-c-py` | `polyglot-c-py__NnoANdu` | 0.0 | rejected | provider | no usable response | — |
| `polyglot-c-py` | `polyglot-c-py__SzVEbgj` | 0.0 | rejected | provider | no usable response | — |
| `polyglot-c-py` | `polyglot-c-py__X3fs9aB` | U | rejected | provider | no usable response | RuntimeError |
| `polyglot-rust-c` | `polyglot-rust-c__3hA3kGz` | 0.0 | accepted | — | — | — |
| `polyglot-rust-c` | `polyglot-rust-c__3nXohER` | 0.0 | accepted | — | — | — |
| `polyglot-rust-c` | `polyglot-rust-c__EWL8xpT` | 0.0 | rejected | provider | no usable response | — |
| `polyglot-rust-c` | `polyglot-rust-c__LaT2DCC` | 0.0 | rejected | provider | no usable response | — |
| `portfolio-optimization` | `portfolio-optimization__FUjWqST` | U | accepted | — | — | RuntimeError |
| `portfolio-optimization` | `portfolio-optimization__Shmqvhc` | 1.0 | rejected | provider | no usable response | — |
| `portfolio-optimization` | `portfolio-optimization__WQCyVV5` | 1.0 | rejected | provider | no usable response | — |
| `protein-assembly` | `protein-assembly__QKqRB2d` | 0.0 | rejected | provider | no usable response | — |
| `protein-assembly` | `protein-assembly__TRCJ7oZ` | 0.0 | accepted | — | — | — |
| `protein-assembly` | `protein-assembly__bV2ZPMo` | 0.0 | rejected | provider | no usable response | — |
| `protein-assembly` | `protein-assembly__oBD9FcJ` | 0.0 | rejected | provider | no usable response | AgentTimeoutError |
| `protein-assembly` | `protein-assembly__yToET7c` | 0.0 | rejected | provider | no usable response | — |
| `prove-plus-comm` | `prove-plus-comm__2cXHrFF` | U | missing | — | — | RuntimeError |
| `prove-plus-comm` | `prove-plus-comm__dPRZXUj` | U | missing | — | — | RuntimeError |
| `prove-plus-comm` | `prove-plus-comm__fSyesj9` | U | missing | — | — | RuntimeError |
| `pypi-server` | `pypi-server__YvRjseG` | 1.0 | rejected | provider | no usable response | — |
| `pypi-server` | `pypi-server__hKmV7vd` | 0.0 | rejected | provider | no usable response | — |
| `pypi-server` | `pypi-server__ssDcR3o` | 0.0 | accepted | — | — | — |
| `pytorch-model-cli` | `pytorch-model-cli__5pJ6NJt` | 1.0 | rejected | provider | image capability | — |
| `pytorch-model-cli` | `pytorch-model-cli__Mta9uVw` | 1.0 | rejected | provider | image capability | — |
| `pytorch-model-cli` | `pytorch-model-cli__Qksg9jP` | U | rejected | provider | image capability | RuntimeError |
| `pytorch-model-recovery` | `pytorch-model-recovery__BbgLWyJ` | 0.0 | rejected | provider | no usable response | — |
| `pytorch-model-recovery` | `pytorch-model-recovery__HGMcWfw` | U | accepted | — | — | VerifierTimeoutError |
| `pytorch-model-recovery` | `pytorch-model-recovery__nizSsXU` | U | rejected | provider | no usable response | VerifierTimeoutError |
| `pytorch-model-recovery` | `pytorch-model-recovery__yXN29am` | U | accepted | — | — | AgentTimeoutError |
| `pytorch-model-recovery` | `pytorch-model-recovery__yZnHYKr` | 0.0 | accepted | — | — | — |
| `qemu-alpine-ssh` | `qemu-alpine-ssh__2EiGiRq` | 0.0 | rejected | provider | no usable response | — |
| `qemu-alpine-ssh` | `qemu-alpine-ssh__5cABxTx` | 0.0 | missing | — | — | AgentTimeoutError |
| `qemu-alpine-ssh` | `qemu-alpine-ssh__7yKmSjM` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `qemu-alpine-ssh` | `qemu-alpine-ssh__a4nCXtq` | 1.0 | missing | — | — | AgentTimeoutError |
| `qemu-alpine-ssh` | `qemu-alpine-ssh__tzUDHMQ` | 0.0 | missing | — | — | AgentTimeoutError |
| `qemu-startup` | `qemu-startup__7Xq8tgq` | U | rejected | provider | no usable response | RuntimeError |
| `qemu-startup` | `qemu-startup__VzYVR7U` | 1.0 | rejected | provider | no usable response | — |
| `qemu-startup` | `qemu-startup__YQKkNsN` | 1.0 | rejected | provider | no usable response | — |
| `query-optimize` | `query-optimize__4vpNRwD` | U | accepted | — | — | RuntimeError |
| `query-optimize` | `query-optimize__ScsesQP` | 0.0 | rejected | provider | no usable response | — |
| `query-optimize` | `query-optimize__UYy58zw` | 1.0 | rejected | provider | no usable response | — |
| `query-optimize` | `query-optimize__bZV4F4Q` | 0.0 | missing | — | — | AgentTimeoutError |
| `query-optimize` | `query-optimize__hbtMdm7` | 0.0 | accepted | — | — | — |
| `raman-fitting` | `raman-fitting__Pkt3WET` | 0.0 | accepted | — | — | — |
| `raman-fitting` | `raman-fitting__TJiztRX` | 0.0 | rejected | provider | no usable response | — |
| `raman-fitting` | `raman-fitting__m5DFW8S` | 0.0 | rejected | provider | no usable response | — |
| `raman-fitting` | `raman-fitting__uMkAsoU` | 0.0 | rejected | provider | no usable response | — |
| `regex-chess` | `regex-chess__BeUr3tT` | 0.0 | rejected | provider | no usable response | — |
| `regex-chess` | `regex-chess__EDMxAJV` | 0.0 | rejected | provider | context limit | — |
| `regex-chess` | `regex-chess__ELH3dX4` | 1.0 | rejected | provider | no usable response | — |
| `regex-chess` | `regex-chess__mp4rEJs` | 1.0 | rejected | provider | no usable response | — |
| `regex-chess` | `regex-chess__nRygVtr` | 0.0 | rejected | provider | context limit | — |
| `regex-log` | `regex-log__fXuZKpf` | 1.0 | rejected | provider | no usable response | — |
| `reshard-c4-data` | `reshard-c4-data__7dPaNBu` | 0.0 | rejected | repository | artifact inventory bound | — |
| `reshard-c4-data` | `reshard-c4-data__RpaHrRb` | U | missing | — | — | RuntimeError |
| `reshard-c4-data` | `reshard-c4-data__SDNqbAD` | U | missing | — | — | RuntimeError |
| `reshard-c4-data` | `reshard-c4-data__twiPQrR` | 0.0 | rejected | repository | artifact inventory bound | — |
| `reshard-c4-data` | `reshard-c4-data__zng8fgS` | U | missing | — | — | RuntimeError |
| `rstan-to-pystan` | `rstan-to-pystan__7ydDPEW` | 1.0 | rejected | provider | no usable response | — |
| `rstan-to-pystan` | `rstan-to-pystan__bYbHErR` | 0.0 | rejected | provider | no usable response | — |
| `rstan-to-pystan` | `rstan-to-pystan__fMdVnzV` | 0.0 | rejected | provider | no usable response | — |
| `sam-cell-seg` | `sam-cell-seg__7ijiR9f` | U | rejected | provider | no usable response | RuntimeError |
| `sam-cell-seg` | `sam-cell-seg__8XMD3nv` | 0.0 | rejected | provider | image capability | — |
| `sam-cell-seg` | `sam-cell-seg__cfbZLR2` | 0.0 | rejected | provider | image capability | — |
| `sam-cell-seg` | `sam-cell-seg__eppTSnp` | 0.0 | accepted | — | — | — |
| `sam-cell-seg` | `sam-cell-seg__g754CXE` | 0.0 | accepted | — | — | — |
| `sanitize-git-repo` | `sanitize-git-repo__a89qRj2` | 1.0 | rejected | provider | no usable response | — |
| `sanitize-git-repo` | `sanitize-git-repo__j2V4XfW` | 1.0 | missing | — | — | AgentTimeoutError |
| `sanitize-git-repo` | `sanitize-git-repo__jG9xXuz` | 1.0 | missing | — | — | AgentTimeoutError |
| `sanitize-git-repo` | `sanitize-git-repo__mY3eqAq` | 1.0 | missing | — | — | AgentTimeoutError |
| `sanitize-git-repo` | `sanitize-git-repo__vutfCYC` | 1.0 | missing | — | — | AgentTimeoutError |
| `schemelike-metacircular-eval` | `schemelike-metacircular-eval__AKffZKy` | 1.0 | rejected | gate | unchanged fixer cycles | — |
| `schemelike-metacircular-eval` | `schemelike-metacircular-eval__LnUT5L8` | 1.0 | rejected | provider | no usable response | — |
| `schemelike-metacircular-eval` | `schemelike-metacircular-eval__tMSBCh9` | 1.0 | rejected | provider | no usable response | — |
| `sparql-university` | `sparql-university__PEjTkw3` | 1.0 | rejected | provider | no usable response | — |
| `sparql-university` | `sparql-university__ZZHfujm` | 1.0 | rejected | provider | no usable response | — |
| `sparql-university` | `sparql-university__tc2iYq7` | 1.0 | rejected | provider | no usable response | — |
| `sqlite-db-truncate` | `sqlite-db-truncate__EsszwjW` | 1.0 | rejected | provider | no usable response | — |
| `sqlite-db-truncate` | `sqlite-db-truncate__Z5SgEwp` | U | accepted | — | — | RuntimeError |
| `sqlite-db-truncate` | `sqlite-db-truncate__vvgCxxU` | 1.0 | rejected | provider | no usable response | — |
| `sqlite-with-gcov` | `sqlite-with-gcov__Y5TiN5C` | 1.0 | rejected | provider | context limit | — |
| `sqlite-with-gcov` | `sqlite-with-gcov__m9wyNWb` | 0.0 | rejected | waiting_for_user | toolchain handoff | — |
| `sqlite-with-gcov` | `sqlite-with-gcov__oVCSMPT` | 1.0 | rejected | provider | context limit | — |
| `sqlite-with-gcov` | `sqlite-with-gcov__rmYaLST` | 1.0 | missing | — | — | AgentTimeoutError |
| `sqlite-with-gcov` | `sqlite-with-gcov__zsm9FAH` | 0.0 | rejected | waiting_for_user | toolchain handoff | — |
| `torch-pipeline-parallelism` | `torch-pipeline-parallelism__4w6bFRU` | U | missing | — | — | AgentTimeoutError |
| `torch-pipeline-parallelism` | `torch-pipeline-parallelism__Ji5Bu4C` | U | rejected | gate | unchanged fixer cycles | VerifierTimeoutError |
| `torch-pipeline-parallelism` | `torch-pipeline-parallelism__LwPjbQF` | U | rejected | provider | no usable response | VerifierTimeoutError |
| `torch-pipeline-parallelism` | `torch-pipeline-parallelism__TkjoAR9` | U | rejected | provider | no usable response | VerifierTimeoutError |
| `torch-pipeline-parallelism` | `torch-pipeline-parallelism__cxjXuqN` | U | rejected | gate | unchanged fixer cycles | AgentTimeoutError |
| `torch-tensor-parallelism` | `torch-tensor-parallelism__CrfdDzK` | 0.0 | missing | — | — | AgentTimeoutError |
| `torch-tensor-parallelism` | `torch-tensor-parallelism__MaL3XdA` | 0.0 | rejected | gate | unchanged fixer cycles | — |
| `torch-tensor-parallelism` | `torch-tensor-parallelism__QzhvGPy` | U | rejected | provider | no usable response | VerifierTimeoutError |
| `torch-tensor-parallelism` | `torch-tensor-parallelism__nvQ4nLS` | 0.0 | rejected | provider | no usable response | — |
| `torch-tensor-parallelism` | `torch-tensor-parallelism__zbXZD9W` | 0.0 | missing | — | — | AgentTimeoutError |
| `train-fasttext` | `train-fasttext__DAiKUJQ` | 0.0 | missing | — | — | AgentTimeoutError |
| `train-fasttext` | `train-fasttext__UFm6sCK` | 0.0 | missing | — | — | AgentTimeoutError |
| `train-fasttext` | `train-fasttext__gzBdSni` | 0.0 | rejected | provider | no usable response | — |
| `train-fasttext` | `train-fasttext__wQg7iUV` | 0.0 | missing | — | — | AgentTimeoutError |
| `tune-mjcf` | `tune-mjcf__56dpLe6` | 1.0 | rejected | provider | no usable response | — |
| `tune-mjcf` | `tune-mjcf__8PJ7Fmd` | 0.0 | rejected | provider | provider process timeout | — |
| `video-processing` | `video-processing__4bTdZmN` | 0.0 | rejected | provider | no usable response | — |
| `video-processing` | `video-processing__4dvBUc4` | 0.0 | rejected | provider | no usable response | — |
| `video-processing` | `video-processing__FprWpSY` | 0.0 | accepted | — | — | — |
| `video-processing` | `video-processing__fdAbXGX` | U | rejected | provider | no usable response | RuntimeError |
| `vulnerable-secret` | `vulnerable-secret__aoXTUBZ` | 0.0 | rejected | provider | authentication | — |
| `vulnerable-secret` | `vulnerable-secret__ddRHVBX` | 0.0 | rejected | provider | no usable response | — |
| `vulnerable-secret` | `vulnerable-secret__iegyEDc` | 0.0 | rejected | provider | no usable response | — |
| `vulnerable-secret` | `vulnerable-secret__uXHVimA` | 0.0 | rejected | provider | no usable response | — |
| `winning-avg-corewars` | `winning-avg-corewars__CZWhwbn` | 1.0 | missing | — | — | AgentTimeoutError |
| `winning-avg-corewars` | `winning-avg-corewars__DcRTVhp` | 1.0 | rejected | provider | no usable response | — |
| `winning-avg-corewars` | `winning-avg-corewars__FmC3fWA` | 1.0 | rejected | provider | no usable response | — |
| `winning-avg-corewars` | `winning-avg-corewars__VjzHSLy` | 1.0 | rejected | provider | no usable response | — |
| `write-compressor` | `write-compressor__6EdgJVj` | 1.0 | rejected | provider | no usable response | — |
| `write-compressor` | `write-compressor__7cYF9nQ` | 1.0 | missing | — | — | AgentTimeoutError |
