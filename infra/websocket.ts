import * as aws from "@pulumi/aws";

export const wsConnectionsTable = new aws.dynamodb.Table("afterstay-ws-connections", {
  name: "afterstay-ws-connections",
  hashKey: "connectionId",
  billingMode: "PAY_PER_REQUEST",
  attributes: [
    { name: "connectionId", type: "S" },
    { name: "tripId", type: "S" },
  ],
  globalSecondaryIndexes: [
    {
      name: "tripId-index",
      hashKey: "tripId",
      projectionType: "ALL",
    },
  ],
  ttl: {
    attributeName: "ttl",
    enabled: true,
  },
});
