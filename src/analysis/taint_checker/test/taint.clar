;;RUN: clarinet check | filecheck %s

(define-public (tainted (amount uint))
;; CHECK: test:10:20: warning: use of potentially tainted data
;; CHECK-NEXT:     (stx-transfer? amount (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                    ^~~~~~
;; CHECK-NEXT: test:3:25: note: source of taint here
;; CHECK-NEXT: (define-public (tainted (amount uint))
;; CHECK-NEXT:                          ^~~~~~
    (stx-transfer? amount (as-contract tx-sender) tx-sender)
)

(define-public (expr-tainted (amount uint))
;; CHECK: test:20:20: warning: use of potentially tainted data
;; CHECK-NEXT:     (stx-transfer? (+ u10 amount) (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                    ^~~~~~~~~~~~~~
;; CHECK-NEXT: test:13:30: note: source of taint here
;; CHECK-NEXT: (define-public (expr-tainted (amount uint))
;; CHECK-NEXT:                               ^~~~~~
    (stx-transfer? (+ u10 amount) (as-contract tx-sender) tx-sender)
)

(define-public (let-tainted (amount uint))
    (let ((x amount))
;; CHECK: test:31:24: warning: use of potentially tainted data
;; CHECK-NEXT:         (stx-transfer? x (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                        ^
;; CHECK-NEXT: test:23:29: note: source of taint here
;; CHECK-NEXT: (define-public (let-tainted (amount uint))
;; CHECK-NEXT:                              ^~~~~~
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (filtered (amount uint))
    (begin
        (asserts! (< amount u100) (err u100))
;; CHECK-NOT: test:39:24: warning:
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)

(define-public (filtered-expr (amount uint))
    (begin
        (asserts! (< (+ amount u10) u100) (err u100))
;; CHECK-NOT: test:47:24: warning:
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-filtered (amount uint))
    (let ((x amount))
        (asserts! (< x u100) (err u100))
;; CHECK-NOT: test:55:24: warning:
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-filtered-parent (amount uint))
    (let ((x amount))
        (asserts! (< amount u100) (err u100))
;; CHECK-NOT: test:63:24: warning:
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
;; CHECK: test:78:24: warning: use of potentially tainted data
;; CHECK-NEXT:         (stx-transfer? x (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                        ^
;; CHECK-NEXT: test:67:35: note: source of taint here
;; CHECK-NEXT: (define-public (let-tainted-twice (amount1 uint) (amount2 uint))
;; CHECK-NEXT:                                    ^~~~~~~
;; CHECK-NEXT: test:67:50: note: source of taint here
;; CHECK-NEXT: (define-public (let-tainted-twice (amount1 uint) (amount2 uint))
;; CHECK-NEXT:                                                   ^~~~~~~
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-once (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
;; CHECK: test:91:24: warning: use of potentially tainted data
;; CHECK-NEXT:         (stx-transfer? x (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                        ^
;; CHECK-NEXT: test:82:64: note: source of taint here
;; CHECK-NEXT: (define-public (let-tainted-twice-filtered-once (amount1 uint) (amount2 uint))
;; CHECK-NEXT:                                                                 ^~~~~~~
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
        (asserts! (< amount2 u100) (err u101))
;; CHECK-NOT: test:100:24: warning:
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-together (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< (+ amount1 amount2) u100) (err u100))
;; CHECK-NOT: test:108:24: warning:
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (if-filter (amount uint))
;; CHECK-NOT: test:114:40: warning:
    (stx-transfer? (if (< amount u100) amount u100) (as-contract tx-sender) tx-sender)
)

(define-public (if-not-filtered (amount uint))
;; CHECK: test:124:20: warning: use of potentially tainted data
;; CHECK-NEXT:     (stx-transfer? (if (< u50 u100) amount u100) (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                    ^~~~~~~~~~~~~~~~~~~~~~~~~~~~~
;; CHECK-NEXT: test:117:33: note: source of taint here
;; CHECK-NEXT: (define-public (if-not-filtered (amount uint))
;; CHECK-NEXT:                                  ^~~~~~
    (stx-transfer? (if (< u50 u100) amount u100) (as-contract tx-sender) tx-sender)
)

(define-public (and-tainted (amount uint))
    (ok (and
;; CHECK: test:135:38: warning: use of potentially tainted data
;; CHECK-NEXT:         (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
;; CHECK-NEXT:                                      ^~~~~~
;; CHECK-NEXT: test:127:29: note: source of taint here
;; CHECK-NEXT: (define-public (and-tainted (amount uint))
;; CHECK-NEXT:                              ^~~~~~
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (and-filter (amount uint))
    (ok (and
        (< amount u100)
;; CHECK-NOT: test:143:38: warning:
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (and-filter-after (amount uint))
    (ok (and
;; CHECK: test:155:38: warning: use of potentially tainted data
;; CHECK-NEXT:         (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
;; CHECK-NEXT:                                      ^~~~~~
;; CHECK-NEXT: test:147:34: note: source of taint here
;; CHECK-NEXT: (define-public (and-filter-after (amount uint))
;; CHECK-NEXT:                                   ^~~~~~
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (< amount u100)
    ))
)

(define-public (or-tainted (amount uint))
    (ok (or
;; CHECK: test:168:38: warning: use of potentially tainted data
;; CHECK-NEXT:         (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
;; CHECK-NEXT:                                      ^~~~~~
;; CHECK-NEXT: test:160:28: note: source of taint here
;; CHECK-NEXT: (define-public (or-tainted (amount uint))
;; CHECK-NEXT:                             ^~~~~~
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (or-filter (amount uint))
    (ok (or
        (>= amount u100)
;; CHECK-NOT: test:166:38: warning:
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (or-filter-after (amount uint))
    (ok (or
;; CHECK: test:188:38: warning: use of potentially tainted data
;; CHECK-NEXT:         (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
;; CHECK-NEXT:                                      ^~~~~~
;; CHECK-NEXT: test:180:33: note: source of taint here
;; CHECK-NEXT: (define-public (or-filter-after (amount uint))
;; CHECK-NEXT:                                  ^~~~~~
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (>= amount u100)
    ))
)

(define-public (tainted-stx-burn (amount uint))
;; CHECK: test:200:16: warning: use of potentially tainted data
;; CHECK-NEXT:     (stx-burn? amount (as-contract tx-sender))
;; CHECK-NEXT:                ^~~~~~
;; CHECK-NEXT: test:193:34: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-stx-burn (amount uint))
;; CHECK-NEXT:                                   ^~~~~~
    (stx-burn? amount (as-contract tx-sender))
)

(define-fungible-token stackaroo)

(define-public (tainted-ft-burn (amount uint))
;; CHECK: test:212:25: warning: use of potentially tainted data
;; CHECK-NEXT:     (ft-burn? stackaroo amount (as-contract tx-sender))
;; CHECK-NEXT:                         ^~~~~~
;; CHECK-NEXT: test:205:33: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-ft-burn (amount uint))
;; CHECK-NEXT:                                  ^~~~~~
    (ft-burn? stackaroo amount (as-contract tx-sender))
)

(define-public (tainted-ft-transfer (amount uint))
;; CHECK: test:222:29: warning: use of potentially tainted data
;; CHECK-NEXT:     (ft-transfer? stackaroo amount (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                             ^~~~~~
;; CHECK-NEXT: test:215:37: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-ft-transfer (amount uint))
;; CHECK-NEXT:                                      ^~~~~~
    (ft-transfer? stackaroo amount (as-contract tx-sender) tx-sender)
)

(define-public (tainted-ft-mint (amount uint))
;; CHECK: test:232:25: warning: use of potentially tainted data
;; CHECK-NEXT:     (ft-mint? stackaroo amount (as-contract tx-sender))
;; CHECK-NEXT:                         ^~~~~~
;; CHECK-NEXT: test:225:33: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-ft-mint (amount uint))
;; CHECK-NEXT:                                  ^~~~~~
    (ft-mint? stackaroo amount (as-contract tx-sender))
)

(define-non-fungible-token stackaroo2 uint)

(define-public (tainted-nft-burn (amount uint))
;; CHECK: test:244:27: warning: use of potentially tainted data
;; CHECK-NEXT:     (nft-burn? stackaroo2 amount (as-contract tx-sender))
;; CHECK-NEXT:                           ^~~~~~
;; CHECK-NEXT: test:237:34: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-nft-burn (amount uint))
;; CHECK-NEXT:                                   ^~~~~~
    (nft-burn? stackaroo2 amount (as-contract tx-sender))
)

(define-public (tainted-nft-transfer (amount uint))
;; CHECK: test:254:31: warning: use of potentially tainted data
;; CHECK-NEXT:     (nft-transfer? stackaroo2 amount (as-contract tx-sender) tx-sender)
;; CHECK-NEXT:                               ^~~~~~
;; CHECK-NEXT: test:247:38: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-nft-transfer (amount uint))
;; CHECK-NEXT:                                       ^~~~~~
    (nft-transfer? stackaroo2 amount (as-contract tx-sender) tx-sender)
)

(define-public (tainted-nft-mint (amount uint))
;; CHECK: test:264:27: warning: use of potentially tainted data
;; CHECK-NEXT:     (nft-mint? stackaroo2 amount (as-contract tx-sender))
;; CHECK-NEXT:                           ^~~~~~
;; CHECK-NEXT: test:257:34: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-nft-mint (amount uint))
;; CHECK-NEXT:                                   ^~~~~~
    (nft-mint? stackaroo2 amount (as-contract tx-sender))
)

(define-data-var myvar uint u0)

(define-public (tainted-var-set (amount uint))
;; CHECK: test:276:24: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (var-set myvar amount))
;; CHECK-NEXT:                        ^~~~~~
;; CHECK-NEXT: test:269:33: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-var-set (amount uint))
;; CHECK-NEXT:                                 ^~~~~~
    (ok (var-set myvar amount))
)

(define-map mymap { key-name-1: uint } { val-name-1: int })

(define-public (tainted-map-set (key uint) (value int))
;; CHECK: test:294:37: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (map-set mymap {key-name-1: key} {val-name-1: value}))
;; CHECK-NEXT:                                     ^~~
;; CHECK-NEXT: test:281:33: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-map-set (key uint) (value int))
;; CHECK-NEXT:                                  ^~~
;; CHECK-NEXT: test:294:55: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (map-set mymap {key-name-1: key} {val-name-1: value}))
;; CHECK-NEXT:                                                       ^~~~~
;; CHECK-NEXT: test:281:44: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-map-set (key uint) (value int))
;; CHECK-NEXT:                                             ^~~~~
    (ok (map-set mymap {key-name-1: key} {val-name-1: value}))
)

(define-public (tainted-map-insert (key uint) (value int))
;; CHECK: test:310:40: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))
;; CHECK-NEXT:                                        ^~~
;; CHECK-NEXT: test:297:36: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-map-insert (key uint) (value int))
;; CHECK-NEXT:                                     ^~~
;; CHECK-NEXT: test:310:58: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))
;; CHECK-NEXT:                                                          ^~~~~
;; CHECK-NEXT: test:297:47: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-map-insert (key uint) (value int))
;; CHECK-NEXT:                                                ^~~~~
    (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))
)

(define-public (tainted-map-delete (key uint))
;; CHECK: test:320:40: warning: use of potentially tainted data
;; CHECK-NEXT:     (ok (map-delete mymap {key-name-1: key}))
;; CHECK-NEXT:                                        ^~~
;; CHECK-NEXT: test:313:36: note: source of taint here
;; CHECK-NEXT: (define-public (tainted-map-delete (key uint))
;; CHECK-NEXT:                                     ^~~
    (ok (map-delete mymap {key-name-1: key}))
)
