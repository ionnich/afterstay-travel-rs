import * as aws from "@pulumi/aws";
import * as pulumi from "@pulumi/pulumi";

import { vpc, privateSubnetIds } from "./network";
import { dbSecret, anthropicSecret, googlePlacesSecret, weatherSecret } from "./db";
import { mediaBucket } from "./storage";
import { userPoolId } from "./cognito";
import { ecrRepos } from "./ecr";

const REGION = "ap-southeast-1";
const ACCOUNT = "755251749545";

const config = new pulumi.Config();
const imageTag = config.get("imageTag") ?? "dev";

// IAM role shared by all three Lambdas.
const role = new aws.iam.Role("afterstay-lambda-role", {
  assumeRolePolicy: JSON.stringify({
    Version: "2012-10-17",
    Statement: [
      {
        Action: "sts:AssumeRole",
        Effect: "Allow",
        Principal: { Service: "lambda.amazonaws.com" },
      },
    ],
  }),
  managedPolicyArns: [
    "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole",
    "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole",
  ],
});

new aws.iam.RolePolicy("afterstay-lambda-policy", {
  role: role.id,
  policy: pulumi
    .all([
      dbSecret.arn,
      anthropicSecret.arn,
      googlePlacesSecret.arn,
      weatherSecret.arn,
      mediaBucket.bucket,
    ])
    .apply(([dbArn, anthropicArn, placesArn, weatherArn, bucket]) =>
      JSON.stringify({
        Version: "2012-10-17",
        Statement: [
          {
            Effect: "Allow",
            Action: ["secretsmanager:GetSecretValue"],
            Resource: [dbArn, anthropicArn, placesArn, weatherArn],
          },
          {
            Effect: "Allow",
            Action: ["s3:PutObject", "s3:GetObject"],
            Resource: [`arn:aws:s3:::${bucket}/*`],
          },
          {
            Effect: "Allow",
            Action: ["dynamodb:*"],
            Resource: `arn:aws:dynamodb:${REGION}:${ACCOUNT}:table/afterstay-ws-connections`,
          },
        ],
      }),
    ),
});

// Security group: reach RDS + internet via NAT.
const sg = new aws.ec2.SecurityGroup("afterstay-lambda-sg", {
  vpcId: vpc.id,
  egress: [
    {
      protocol: "-1",
      fromPort: 0,
      toPort: 0,
      cidrBlocks: ["0.0.0.0/0"],
    },
  ],
});

const crates = ["api", "integrations", "chat"] as const;
type Crate = (typeof crates)[number];

const lambdas: Record<Crate, aws.lambda.Function> = {} as Record<Crate, aws.lambda.Function>;

for (const crate of crates) {
  lambdas[crate] = new aws.lambda.Function(`afterstay-${crate}`, {
    packageType: "Image",
    imageUri: pulumi.interpolate`${ecrRepos[crate]}:${imageTag}`,
    architectures: ["arm64"],
    role: role.arn,
    memorySize: 512,
    timeout: 30,
    vpcConfig: {
      subnetIds: privateSubnetIds,
      securityGroupIds: [sg.id],
    },
    environment: {
      variables: {
        DB_SECRET_ID: dbSecret.arn,
        COGNITO_USER_POOL_ID: userPoolId,
        MEDIA_BUCKET: mediaBucket.bucket,
        WS_TABLE: "afterstay-ws-connections",
      },
    },
  });
}

// ---- HTTP API ----
const api = new aws.apigatewayv2.Api("afterstay-api", {
  protocolType: "HTTP",
});

function httpRoute(crate: "api" | "integrations", routeKey: string) {
  const fn = lambdas[crate];

  const integration = new aws.apigatewayv2.Integration(`afterstay-api-${crate}-integration`, {
    apiId: api.id,
    integrationType: "AWS_PROXY",
    integrationUri: fn.invokeArn,
    payloadFormatVersion: "2.0",
  });

  new aws.apigatewayv2.Route(`afterstay-api-${crate}-route`, {
    apiId: api.id,
    routeKey,
    target: pulumi.interpolate`integrations/${integration.id}`,
  });

  new aws.lambda.Permission(`afterstay-api-${crate}-permission`, {
    action: "lambda:InvokeFunction",
    function: fn.name,
    principal: "apigateway.amazonaws.com",
    sourceArn: pulumi.interpolate`${api.executionArn}/*/*`,
  });
}

httpRoute("api", "ANY /v1/data/{proxy+}");
httpRoute("integrations", "ANY /v1/integrations/{proxy+}");

const stage = new aws.apigatewayv2.Stage("afterstay-api-stage", {
  apiId: api.id,
  name: "v1",
  autoDeploy: true,
});

export const apiUrl = stage.invokeUrl;

// ---- WebSocket API ----
const wsApi = new aws.apigatewayv2.Api("afterstay-ws", {
  protocolType: "WEBSOCKET",
  routeSelectionExpression: "$request.body.action",
});

const wsIntegration = new aws.apigatewayv2.Integration("afterstay-ws-chat-integration", {
  apiId: wsApi.id,
  integrationType: "AWS_PROXY",
  integrationUri: lambdas.chat.invokeArn,
});

for (const routeKey of ["$connect", "$disconnect", "$default"]) {
  new aws.apigatewayv2.Route(`afterstay-ws-${routeKey.replace("$", "")}`, {
    apiId: wsApi.id,
    routeKey,
    target: pulumi.interpolate`integrations/${wsIntegration.id}`,
  });
}

new aws.lambda.Permission("afterstay-ws-permission", {
  action: "lambda:InvokeFunction",
  function: lambdas.chat.name,
  principal: "apigateway.amazonaws.com",
  sourceArn: pulumi.interpolate`${wsApi.executionArn}/*/*`,
});

const wsStage = new aws.apigatewayv2.Stage("afterstay-ws-stage", {
  apiId: wsApi.id,
  name: "prod",
  autoDeploy: true,
});

export const wsUrl = wsStage.invokeUrl;
