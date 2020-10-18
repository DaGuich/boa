//! Execution module for the test runner.

use super::{
    Harness, Outcome, Phase, SuiteResult, Test, TestFlags, TestOutcomeResult, TestResult, TestSuite,
};
use boa::{parse, Context};
use colored::Colorize;
use fxhash::FxHashSet;
use once_cell::sync::Lazy;
use std::{fs, panic, path::Path};

/// List of ignored tests.
static IGNORED: Lazy<FxHashSet<Box<str>>> = Lazy::new(|| {
    let path = Path::new("test_ignore.txt");
    if path.exists() {
        let filtered = fs::read_to_string(path).expect("could not read test filters");
        filtered
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .map(|line| line.to_owned().into_boxed_str())
            .collect::<FxHashSet<_>>()
    } else {
        FxHashSet::default()
    }
});

impl TestSuite {
    /// Runs the test suite.
    pub(crate) fn run(&self, harness: &Harness, verbose: u8) -> SuiteResult {
        if verbose != 0 {
            println!("Suite {}:", self.name);
        }

        // TODO: in parallel
        let suites: Vec<_> = self
            .suites
            .iter()
            .map(|suite| suite.run(harness, verbose))
            .collect();

        // TODO: in parallel
        let tests: Vec<_> = self
            .tests
            .iter()
            .map(|test| test.run(harness, verbose))
            .flatten()
            .collect();

        if verbose != 0 {
            println!();
        }

        // Count passed tests
        let mut passed = 0;
        let mut ignored = 0;
        let mut panic = 0;
        for test in &tests {
            match test.result {
                TestOutcomeResult::Passed => passed += 1,
                TestOutcomeResult::Ignored => ignored += 1,
                TestOutcomeResult::Panic => panic += 1,
                TestOutcomeResult::Failed => {}
            }
        }

        // Count total tests
        let mut total = tests.len();
        for suite in &suites {
            total += suite.total;
            passed += suite.passed;
            ignored += suite.ignored;
            panic += suite.panic;
        }

        if verbose != 0 {
            println!(
                "Results: total: {}, passed: {}, ignored: {}, panics: {}, conformance: {:.2}%",
                total,
                passed.to_string().green(),
                ignored.to_string().yellow(),
                panic.to_string().red(),
                (passed as f64 / total as f64) * 100.0
            );
        }

        SuiteResult {
            name: self.name.clone(),
            total,
            passed,
            ignored,
            panic,
            suites,
            tests,
        }
    }
}

impl Test {
    /// Runs the test.
    pub(crate) fn run(&self, harness: &Harness, verbose: u8) -> Vec<TestResult> {
        let mut results = Vec::new();
        if self.flags.contains(TestFlags::STRICT) {
            results.push(self.run_once(harness, true, verbose));
        }

        if self.flags.contains(TestFlags::NO_STRICT) || self.flags.contains(TestFlags::RAW) {
            results.push(self.run_once(harness, false, verbose));
        }

        results
    }

    /// Runs the test once, in strict or non-strict mode
    fn run_once(&self, harness: &Harness, strict: bool, verbose: u8) -> TestResult {
        if verbose > 1 {
            println!(
                "Starting `{}`{}",
                self.name,
                if strict { " (strict mode)" } else { "" }
            );
        }

        let (result, result_text) = if !self.flags.intersects(TestFlags::ASYNC | TestFlags::MODULE)
            && !IGNORED.contains(&self.name)
            && (matches!(self.expected_outcome, Outcome::Positive)
                || matches!(self.expected_outcome, Outcome::Negative {
                    phase: Phase::Parse,
                    error_type: _,
                })) {
            let res = panic::catch_unwind(|| match self.expected_outcome {
                Outcome::Positive => {
                    let mut engine = self.set_up_env(&harness, strict);
                    let res = engine.eval(&self.content);

                    let passed = res.is_ok();
                    let text = match res {
                        Ok(val) => format!("{}", val.display()),
                        Err(e) => format!("Uncaught {}", e.display()),
                    };

                    (passed, text)
                }
                Outcome::Negative {
                    phase: Phase::Parse,
                    ref error_type,
                } => {
                    assert_eq!(
                        error_type.as_ref(),
                        "SyntaxError",
                        "non-SyntaxError parsing error found in {}",
                        self.name
                    );

                    match parse(&self.content, strict) {
                        Ok(n) => (false, format!("{:?}", n)),
                        Err(e) => (true, format!("Uncaught {}", e)),
                    }
                }
                Outcome::Negative {
                    phase: _,
                    error_type: _,
                } => todo!("check the phase"),
            });

            let result = res
                .map(|(res, text)| {
                    if res {
                        (TestOutcomeResult::Passed, text)
                    } else {
                        (TestOutcomeResult::Failed, text)
                    }
                })
                .unwrap_or_else(|_| {
                    eprintln!("last panic was on test \"{}\"", self.name);
                    (TestOutcomeResult::Panic, String::new())
                });

            print!(
                "{}",
                if let (TestOutcomeResult::Passed, _) = result {
                    ".".green()
                } else {
                    ".".red()
                }
            );

            result
        } else {
            // Ignoring async tests for now.
            // TODO: implement async and add `harness/doneprintHandle.js` to the includes.
            print!("{}", ".".yellow());
            (TestOutcomeResult::Ignored, String::new())
        };

        TestResult {
            name: self.name.clone(),
            strict,
            result,
            result_text: result_text.into_boxed_str(),
        }
    }

    /// Sets the environment up to run the test.
    fn set_up_env(&self, harness: &Harness, strict: bool) -> Context {
        // Create new Realm
        // TODO: in parallel.
        let mut engine = Context::new();

        // TODO: set up the environment.

        if strict {
            engine
                .eval(r#""use strict";"#)
                .expect("could not set strict mode");
        }

        engine
            .eval(&harness.assert)
            .expect("could not run assert.js");
        engine.eval(&harness.sta).expect("could not run sta.js");

        self.includes.iter().for_each(|include| {
            let res = engine.eval(
                &harness
                    .includes
                    .get(include)
                    .expect("could not find include file"),
            );
            if let Err(e) = res {
                eprintln!("could not run the {} include file.", include);
                panic!("Uncaught {}", e.display());
            }
        });

        engine
    }
}
