export interface PackageArtifact {
  readonly filename: string;
  readonly integrity: string;
  readonly name: string;
  readonly size: number;
  readonly version: string;
}

export interface PackageManifest {
  readonly packages: readonly PackageArtifact[];
  readonly version: string;
}
