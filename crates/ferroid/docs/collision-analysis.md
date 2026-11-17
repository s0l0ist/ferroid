# Collision Probability Analysis

When generating time-sortable IDs that use random bits, it's important to
estimate the probability of collisions (i.e., two IDs being the same within the
same millisecond), given your ID layout and system throughput.

The calculations below assume millisecond timestamps. If you use a coarser
custom clock, substitute your time-source tick size accordingly.

## Non-monotonic (always-random) collision probability

If $n$ IDs are generated within the same millisecond, and the ID has $r$ random
bits, the probability of **at least one collision** in that millisecond is
approximately:

$$P_\text{collision} \approx \frac{n(n-1)}{2 \cdot 2^r} $$

For $g$ generators each producing $k$ IDs per millisecond (so $n = g \cdot k$):

$$P_\text{collision} \approx \frac{(gk)(gk-1)}{2 \cdot 2^r} $$

Where:

- $g$ = number of generators
- $k$ = number of non-monotonic IDs per generator per millisecond
- $r$ = number of random bits per ID
- $n$ = total IDs generated per millisecond across all generators ($n = g \cdot k$)
- $P_{\text{collision}}$ = probability of at least one collision (within the
  same millisecond)

When a generator emits multiple IDs per tick, always-random generation
introduces within-generator collision risk. Monotonic generation prevents
these collisions by incrementing from its random starting point.

## Monotonic IDs with Multiple ULID Generators

If you have $g$ generators (e.g., distributed nodes), and each generator
produces $k$ **sequential** (monotonic) IDs per millisecond by incrementing from
a random starting point, the probability that any two generators produce
overlapping IDs in the same millisecond is approximately:

$$P_\text{collision} \approx \frac{g(g-1)(2k-1)}{2 \cdot 2^r}$$

Where:

- $g$ = number of generators
- $k$ = number of monotonic IDs per generator per millisecond
- $r$ = number of random bits per ID
- $P_\text{collision}$ = probability of at least one collision

> **Note**: The formula above uses the approximate (birthday bound) model, which
> assumes that:
>
> - $k \ll 2^r$ and $g \ll 2^r$
> - Each generator's range of $k$ IDs starts at a uniformly random position
>   within the $r$-bit space

## Estimating Time Until a Collision Occurs

While collisions only happen within a single millisecond, we often want to know
how long it takes before **any** collision happens, given continuous generation
over time.

Assuming independent collision events across milliseconds, the time in
milliseconds to reach a 50% chance of collision is:

$$
T_{\text{50\%}} \approx \frac{\ln 2}{P_\text{collision}}
$$

Use the per-millisecond probability for the chosen generation strategy. For
monotonic generators, substituting the range-overlap approximation gives:

$$T_{\text{50\%}} \approx \frac{2 \ln 2 \cdot 2^r}{g(g - 1)(2k - 1)}$$

For always-random generators, substitute the birthday approximation instead,
with $n = gk$:

$$T_{\text{50\%}} \approx \frac{2 \ln 2 \cdot 2^r}{n(n - 1)}$$

These approximations require a small per-millisecond collision probability.
When that probability is zero, the time to a 50% chance is infinite under the
stated model.

This is derived from the cumulative probability formula:

$$P_\text{collision}(T) = 1 - (1 - P_\text{collision})^T$$

Solving for $T$ when $P_\text{collision}(T) = 0.5$:

$$(1 - P_\text{collision})^T = 0.5$$ $$\Rightarrow T \approx
\frac{\ln(0.5)}{\ln(1 - P_\text{collision})}$$

Using the approximation $\ln(1 - x) \approx -x$ for small $x$, this simplifies
to:

$$\Rightarrow T \approx \frac{\ln 2}{P_\text{collision}}$$

The $\ln 2$ term arises because $\ln(0.5) = -\ln 2$. After $T_\text{50\%}$
milliseconds, there's a 50% chance that at least one collision has occurred.

## Example Collision Probabilities (Monotonic)

| Generators ($g$) | IDs per generator per ms ($k$) | $P_\text{collision}$                                                                                    | Estimated Time to 50% Collision ($T_{\text{50\%}}$)         |
| ---------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 1                | 1                              | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 1                | 65,536                         | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 2                | 1                              | $\displaystyle \frac{2 \times 1 \times 1}{2 \cdot 2^{80}} \approx 8.27 \times 10^{-25}$                 | $\approx 8.38 \times 10^{23} \text{ ms}$                    |
| 2                | 65,536                         | $\displaystyle \frac{2 \times 1 \times 131{,}071}{2 \cdot 2^{80}} \approx 1.08 \times 10^{-19}$         | $\approx 6.39 \times 10^{18} \text{ ms}$                    |
| 1,000            | 1                              | $\displaystyle \frac{1{,}000 \times 999 \times 1}{2 \cdot 2^{80}} \approx 4.13 \times 10^{-19}$         | $\approx 1.68 \times 10^{18} \text{ ms}$                    |
| 1,000            | 65,536                         | $\displaystyle \frac{1{,}000 \times 999 \times 131{,}071}{2 \cdot 2^{80}} \approx 5.42 \times 10^{-14}$ | $\approx 1.28 \times 10^{13} \text{ ms} \approx 406\ years$ |

## Example Collision Probabilities (Non-Monotonic)

| Generators ($g$) | IDs per generator per ms ($k$) | $P_\text{collision}$                                                                                    | Estimated Time to 50% Collision ($T_{\text{50\%}}$)          |
| ---------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| 1                | 1                              | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                    |
| 1                | 65,536                         | $\displaystyle \frac{65{,}536 \times 65{,}535}{2 \cdot 2^{80}} \approx 1.78 \times 10^{-15}$             | $\approx 3.90 \times 10^{14} \text{ ms}$                     |
| 2                | 1                              | $\displaystyle \frac{2 \times 1}{2 \cdot 2^{80}} \approx 8.27 \times 10^{-25}$                          | $\approx 8.38 \times 10^{23} \text{ ms}$ (same as monotonic) |
| 2                | 65,536                         | $\displaystyle \frac{131{,}072 \times 131{,}071}{2 \cdot 2^{80}} \approx 7.11 \times 10^{-15}$          | $\approx 9.75 \times 10^{13} \text{ ms}$                     |
| 1,000            | 1                              | $\displaystyle \frac{1{,}000 \times 999}{2 \cdot 2^{80}} \approx 4.13 \times 10^{-19}$                  | $\approx 1.68 \times 10^{18} \text{ ms}$ (same as monotonic) |
| 1,000            | 65,536                         | $\displaystyle \frac{65{,}536{,}000 \times 65{,}535{,}999}{2 \cdot 2^{80}} \approx 1.78 \times 10^{-9}$ | $\approx 3.90 \times 10^{8} \text{ ms} \approx 4.5\ days$    |
