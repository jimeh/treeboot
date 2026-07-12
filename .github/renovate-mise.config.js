const monthlySchedule = ["* 0-5 1 * *"];
const updateSchedule =
  process.env.RENOVATE_BYPASS_SCHEDULE === "true" ? null : monthlySchedule;

module.exports = {
  allowedCommands: ["^mise lock rust$"],
  allowedUnsafeExecutions: ["mise"],
  branchPrefix: "renovate-mise/",
  dependencyDashboard: false,
  enabledManagers: ["mise", "rust-toolchain"],
  extends: ["config:recommended", ":disableDependencyDashboard"],
  lockFileMaintenance: {
    enabled: true,
    groupName: "mise lockfile",
    schedule: updateSchedule,
  },
  minimumReleaseAge: "7 days",
  onboarding: false,
  packageRules: [
    {
      groupName: "mise tools",
      groupSlug: "mise-tools",
      matchManagers: ["mise"],
    },
    {
      groupName: "Rust toolchain",
      groupSlug: "rust-toolchain",
      matchManagers: ["rust-toolchain"],
      postUpgradeTasks: {
        commands: ["mise lock rust"],
        executionMode: "update",
        fileFilters: ["mise.lock"],
        installTools: {
          mise: {},
        },
      },
    },
  ],
  requireConfig: "optional",
  prCreation: "immediate",
  schedule: updateSchedule,
  semanticCommitScope: "deps",
  semanticCommitType: "chore",
  semanticCommits: "enabled",
  timezone: "UTC",
};
