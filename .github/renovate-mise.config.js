module.exports = {
  allowedUnsafeExecutions: ["mise"],
  branchPrefix: "renovate-mise/",
  dependencyDashboard: false,
  enabledManagers: ["mise"],
  extends: ["config:recommended"],
  lockFileMaintenance: {
    enabled: true,
    groupName: "mise lockfile",
    schedule: ["* 0-5 1 * *"],
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
  requireConfig: "optional",
  schedule: ["* 0-5 1 * *"],
  semanticCommitScope: "deps",
  semanticCommitType: "chore",
  semanticCommits: "enabled",
  timezone: "UTC",
};
