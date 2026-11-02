import os
import unittest
from unittest import mock

from checks import combine, environment, libafl_execution_counts


class ControllerTests(unittest.TestCase):
    def test_failure_dominates_incomplete(self):
        self.assertEqual(
            combine([{"status": "pass"}, {"status": "incomplete"}, {"status": "fail"}]),
            "fail",
        )

    def test_empty_result_set_is_incomplete(self):
        self.assertEqual(combine([]), "incomplete")

    def test_libafl_execution_counters_are_retained(self):
        output = "executions: 14,249, exec/sec: 6.973k\nexecs: 2.011k\nexecutions: 1.5M"
        self.assertEqual(libafl_execution_counts(output), [14_249, 2_011, 1_500_000])

    def test_environment_disables_ambient_compiler_caches(self):
        inherited = {
            "RUSTC_WRAPPER": "sccache",
            "RUSTC_WORKSPACE_WRAPPER": "wrapper",
            "CCACHE_DISABLE": "0",
            "SCCACHE_DISABLE": "0",
        }
        with mock.patch.dict(os.environ, inherited, clear=True):
            result = environment(2)

        self.assertNotIn("RUSTC_WRAPPER", result)
        self.assertNotIn("RUSTC_WORKSPACE_WRAPPER", result)
        self.assertEqual(result["CCACHE_DISABLE"], "1")
        self.assertEqual(result["SCCACHE_DISABLE"], "1")


if __name__ == "__main__":
    unittest.main()
