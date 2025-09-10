
- syzygy should provide ability to have different kind of effect executors and have mulitple executors at the same time
- each executor use SAME EFFECT TYPE
  - but since we are using enum variant macro, each executor can handle different structs
  - trigger_effect is not needed anymore, because
    executor will have method that will take effect_handler(magic function)
- executor will take care for paraleizm, core number, share nothing pattern etc.
