;;RUN: clarinet check | filecheck %s

(define-public (tainted (amount uint))
;; CHECK: Warning (line: 5, line: 3): Use of potentially tainted data.
    (stx-transfer? amount (as-contract tx-sender) tx-sender)
)

(define-public (expr-tainted (amount uint))
;; CHECK: Warning (line: 10, line: 8): Use of potentially tainted data.
    (stx-transfer? (+ u10 amount) (as-contract tx-sender) tx-sender)
)

(define-public (let-tainted (amount uint))
    (let ((x amount))
;; CHECK: Warning (line: 16, line: 13): Use of potentially tainted data.
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (filtered (amount uint))
    (begin
        (asserts! (< amount u100) (err u100))
;; CHECK-NOT: Warning (line: 24,
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)

(define-public (filtered-expr (amount uint))
    (begin
        (asserts! (< (+ amount u10) u100) (err u100))
;; CHECK-NOT: Warning (line 32,
        (stx-transfer? amount (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-filtered (amount uint))
    (let ((x amount))
        (asserts! (< x u100) (err u100))
;; CHECK-NOT: Warning (line 40,
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-filtered-parent (amount uint))
    (let ((x amount))
        (asserts! (< amount u100) (err u100))
;; CHECK-NOT: Warning (line 48,
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
;; CHECK: Warning (line: 55, line: 52, line: 52): Use of potentially tainted data.
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-once (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
;; CHECK: Warning (line: 63, line: 59): Use of potentially tainted data.
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-twice (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< amount1 u100) (err u100))
        (asserts! (< amount2 u100) (err u101))
;; CHECK-NOT: Warning (line 72,
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (let-tainted-twice-filtered-together (amount1 uint) (amount2 uint))
    (let ((x (+ amount1 amount2)))
        (asserts! (< (+ amount1 amount2) u100) (err u100))
;; CHECK-NOT: Warning (line 80,
        (stx-transfer? x (as-contract tx-sender) tx-sender)
    )
)

(define-public (if-filter (amount uint))
;; CHECK-NOT: Warning (line 86,
    (stx-transfer? (if (< amount u100) amount u100) (as-contract tx-sender) tx-sender)
)

(define-public (if-not-filtered (amount uint))
;; CHECK: Warning (line: 91, line: 89): Use of potentially tainted data.
    (stx-transfer? (if (< u50 u100) amount u100) (as-contract tx-sender) tx-sender)
)

(define-public (and-tainted (amount uint))
    (ok (and
;; CHECK: Warning (line: 97, line: 94): Use of potentially tainted data.
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (and-filter (amount uint))
    (ok (and
        (< amount u100)
;; CHECK-NOT: Warning (line: 105,
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (and-filter-after (amount uint))
    (ok (and
;; CHECK: Warning (line: 112, line: 109): Use of potentially tainted data.
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (< amount u100)
    ))
)

(define-public (or-tainted (amount uint))
    (ok (or
;; CHECK: Warning (line: 120, line: 117): Use of potentially tainted data.
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (or-filter (amount uint))
    (ok (or
        (>= amount u100)
;; CHECK-NOT: Warning (line: 128,
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
    ))
)

(define-public (or-filter-after (amount uint))
    (ok (or
;; CHECK: Warning (line: 135, line: 132): Use of potentially tainted data.
        (unwrap-panic (stx-transfer? amount (as-contract tx-sender) tx-sender))
        (>= amount u100)
    ))
)

(define-public (tainted-stx-burn (amount uint))
;; CHECK: Warning (line: 142, line: 140): Use of potentially tainted data.
    (stx-burn? amount (as-contract tx-sender))
)

(define-fungible-token stackaroo)

(define-public (tainted-ft-burn (amount uint))
;; CHECK: Warning (line: 149, line: 147): Use of potentially tainted data.
    (ft-burn? stackaroo amount (as-contract tx-sender))
)

(define-public (tainted-ft-transfer (amount uint))
;; CHECK: Warning (line: 154, line: 152): Use of potentially tainted data.
    (ft-transfer? stackaroo amount (as-contract tx-sender) tx-sender)
)

(define-public (tainted-ft-mint (amount uint))
;; CHECK: Warning (line: 159, line: 157): Use of potentially tainted data.
    (ft-mint? stackaroo amount (as-contract tx-sender))
)

(define-non-fungible-token stackaroo2 uint)

(define-public (tainted-nft-burn (amount uint))
;; CHECK: Warning (line: 166, line: 164): Use of potentially tainted data.
    (nft-burn? stackaroo2 amount (as-contract tx-sender))
)

(define-public (tainted-nft-transfer (amount uint))
;; CHECK: Warning (line: 171, line: 169): Use of potentially tainted data.
    (nft-transfer? stackaroo2 amount (as-contract tx-sender) tx-sender)
)

(define-public (tainted-nft-mint (amount uint))
;; CHECK: Warning (line: 176, line: 174): Use of potentially tainted data.
    (nft-mint? stackaroo2 amount (as-contract tx-sender))
)

(define-data-var myvar uint u0)

(define-public (tainted-var-set (amount uint))
;; CHECK: Warning (line: 183, line: 181): Use of potentially tainted data.
    (ok (var-set myvar amount))
)

(define-map mymap { key-name-1: uint } { val-name-1: int })

(define-public (tainted-map-set (key uint) (value int))
;; CHECK: Warning (line: 191, line: 188): Use of potentially tainted data.
;; CHECK: Warning (line: 191, line: 188): Use of potentially tainted data.
    (ok (map-set mymap {key-name-1: key} {val-name-1: value}))
)

(define-public (tainted-map-insert (key uint) (value int))
;; CHECK: Warning (line: 197, line: 194): Use of potentially tainted data.
;; CHECK: Warning (line: 197, line: 194): Use of potentially tainted data.
    (ok (map-insert mymap {key-name-1: key} {val-name-1: value}))
)

(define-public (tainted-map-delete (key uint))
;; CHECK: Warning (line: 202, line: 200): Use of potentially tainted data.
    (ok (map-delete mymap {key-name-1: key}))
)
