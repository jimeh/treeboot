const monthlySchedule = ["* 0-5 1 * *"];
const updateSchedule =
  process.env.RENOVATE_BYPASS_SCHEDULE === "true" ? null : monthlySchedule;

module.exports = {
  allowedUnsafeExecutions: ["mise"],
  branchPrefix: "renovate-mise/",
  dependencyDashboard: false,
  enabledManagers: ["mise"],
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
  ],
  prCreation: "immediate",
  requireConfig: "optional",
  schedule: updateSchedule,
  semanticCommitScope: "deps",
  semanticCommitType: "chore",
  semanticCommits: "enabled",
  timezone: "UTC",
};
