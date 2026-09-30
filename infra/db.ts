import * as aws from "@pulumi/aws";
import * as random from "@pulumi/random";
import * as pulumi from "@pulumi/pulumi";
import { vpc, privateSubnetIds } from "./network";

const dbSecurityGroup = new aws.ec2.SecurityGroup("afterstay-db", {
  vpcId: vpc.id,
  ingress: [
    {
      protocol: "tcp",
      fromPort: 5432,
      toPort: 5432,
      cidrBlocks: ["10.0.0.0/16"],
    },
  ],
});

const dbSubnetGroup = new aws.rds.SubnetGroup("afterstay-db-subnet", {
  subnetIds: privateSubnetIds,
});

const dbPassword = new random.RandomPassword("afterstay-db-password", {
  length: 32,
  special: false,
});

const db = new aws.rds.Instance("afterstay-db", {
  engine: "postgres",
  engineVersion: "16.4",
  instanceClass: "db.t4g.small",
  dbName: "afterstay",
  username: "afterstay_admin",
  password: dbPassword.result,
  allocatedStorage: 20,
  storageEncrypted: true,
  dbSubnetGroupName: dbSubnetGroup.name,
  vpcSecurityGroupIds: [dbSecurityGroup.id],
  publiclyAccessible: false,
  multiAz: false,
  deletionProtection: true,
  skipFinalSnapshot: false,
});

export const rdsAddress = db.address;
export const rdsEndpoint = db.endpoint;

const dbSecret = new aws.secretsmanager.Secret("afterstay/db", {
  name: "afterstay/db",
});

new aws.secretsmanager.SecretVersion("afterstay/db/version", {
  secretId: dbSecret.id,
  secretString: pulumi.interpolate`{"username":"afterstay_admin","password":"${dbPassword.result}","host":"${db.address}","port":5432,"dbname":"afterstay","engine":"postgres"}`,
});

// Placeholder secrets — values populated later, no SecretVersion yet.
export const anthropicSecret = new aws.secretsmanager.Secret("afterstay/anthropic", {
  name: "afterstay/anthropic",
});

export const googlePlacesSecret = new aws.secretsmanager.Secret("afterstay/google-places", {
  name: "afterstay/google-places",
});

export const weatherSecret = new aws.secretsmanager.Secret("afterstay/weather", {
  name: "afterstay/weather",
});
