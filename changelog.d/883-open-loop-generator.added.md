- The engine gains an open-loop load generator (`veredictum::perf_gen`):
  interactions arrive on a seeded Poisson process laid over a rate profile
  of constant and ramping segments, a session's later stages wait for the
  previous answer and a drawn think time, every latency runs from the
  intended instant, arrivals are tasks under a bounded in-flight budget,
  and each window records the generator's own lag so a window it could
  not hold reads `generator_bound` and a budget that stays exhausted stops
  the run as saturated.
