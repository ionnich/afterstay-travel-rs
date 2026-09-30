import * as aws from "@pulumi/aws";
import * as pulumi from "@pulumi/pulumi";

const config = new pulumi.Config();

const userPool = new aws.cognito.UserPool("afterstay-users", {
  name: "afterstay-users",
  autoVerifiedAttributes: ["email"],
  usernameAttributes: ["email"],
  schemas: [
    {
      name: "name",
      attributeDataType: "String",
      mutable: true,
      required: false,
    },
  ],
});

const googleProvider = new aws.cognito.IdentityProvider("google", {
  userPoolId: userPool.id,
  providerName: "Google",
  providerType: "Google",
  providerDetails: {
    client_id: config.require("googleClientId"),
    client_secret: config.require("googleClientSecret"),
    authorize_scopes: "openid email profile",
  },
  attributeMapping: {
    email: "email",
    username: "sub",
    name: "name",
  },
});

const userPoolClient = new aws.cognito.UserPoolClient(
  "afterstay-app",
  {
    userPoolId: userPool.id,
    generateSecret: false,
    explicitAuthFlows: [
      "ALLOW_USER_PASSWORD_AUTH",
      "ALLOW_REFRESH_TOKEN_AUTH",
      "ALLOW_USER_SRP_AUTH",
    ],
    supportedIdentityProviders: ["COGNITO", "Google"],
  },
  { dependsOn: [googleProvider] },
);

export const userPoolId = userPool.id;
export const userPoolClientId = userPoolClient.id;
