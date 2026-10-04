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
    allowedOauthFlowsUserPoolClient: true,
    allowedOauthFlows: ["code"],
    allowedOauthScopes: ["openid", "email", "profile"],
    callbackUrls: ["afterstay://auth/callback"],
    logoutUrls: ["afterstay://auth/login"],
  },
  { dependsOn: [googleProvider] },
);

// Hosted-UI domain — Google OAuth redirect target is
// https://<domain>.auth.ap-southeast-1.amazoncognito.com/oauth2/idpresponse
const userPoolDomain = new aws.cognito.UserPoolDomain("afterstay-users-domain", {
  userPoolId: userPool.id,
  domain: "afterstay-users",
});

export const userPoolId = userPool.id;
export const userPoolClientId = userPoolClient.id;
export const oauthDomain = pulumi.interpolate`${userPoolDomain.domain}.auth.ap-southeast-1.amazoncognito.com`;
