import * as aws from "@pulumi/aws";

const azA = "ap-southeast-1a";
const azB = "ap-southeast-1b";

export const vpc = new aws.ec2.Vpc("afterstay-vpc", {
  cidrBlock: "10.0.0.0/16",
  enableDnsHostnames: true,
  enableDnsSupport: true,
});

export const publicSubnetA = new aws.ec2.Subnet("afterstay-public-a", {
  vpcId: vpc.id,
  cidrBlock: "10.0.0.0/24",
  availabilityZone: azA,
  mapPublicIpOnLaunch: true,
});

export const publicSubnetB = new aws.ec2.Subnet("afterstay-public-b", {
  vpcId: vpc.id,
  cidrBlock: "10.0.1.0/24",
  availabilityZone: azB,
  mapPublicIpOnLaunch: true,
});

export const privateSubnetA = new aws.ec2.Subnet("afterstay-private-a", {
  vpcId: vpc.id,
  cidrBlock: "10.0.10.0/24",
  availabilityZone: azA,
});

export const privateSubnetB = new aws.ec2.Subnet("afterstay-private-b", {
  vpcId: vpc.id,
  cidrBlock: "10.0.11.0/24",
  availabilityZone: azB,
});

export const publicSubnetIds = [publicSubnetA.id, publicSubnetB.id];
export const privateSubnetIds = [privateSubnetA.id, privateSubnetB.id];

const internetGateway = new aws.ec2.InternetGateway("afterstay-igw", {
  vpcId: vpc.id,
});

const publicRouteTable = new aws.ec2.RouteTable("afterstay-public-rt", {
  vpcId: vpc.id,
});

new aws.ec2.Route("afterstay-public-route", {
  routeTableId: publicRouteTable.id,
  destinationCidrBlock: "0.0.0.0/0",
  gatewayId: internetGateway.id,
});

for (const [i, subnetId] of publicSubnetIds.entries()) {
  new aws.ec2.RouteTableAssociation(`afterstay-public-rta-${i}`, {
    routeTableId: publicRouteTable.id,
    subnetId,
  });
}

const natEip = new aws.ec2.Eip("afterstay-nat-eip", {
  domain: "vpc",
});

const natGateway = new aws.ec2.NatGateway("afterstay-nat", {
  subnetId: publicSubnetA.id,
  allocationId: natEip.id,
});

const privateRouteTable = new aws.ec2.RouteTable("afterstay-private-rt", {
  vpcId: vpc.id,
});

new aws.ec2.Route("afterstay-private-route", {
  routeTableId: privateRouteTable.id,
  destinationCidrBlock: "0.0.0.0/0",
  natGatewayId: natGateway.id,
});

for (const [i, subnetId] of privateSubnetIds.entries()) {
  new aws.ec2.RouteTableAssociation(`afterstay-private-rta-${i}`, {
    routeTableId: privateRouteTable.id,
    subnetId,
  });
}
