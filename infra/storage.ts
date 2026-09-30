import * as aws from "@pulumi/aws";

export const mediaBucket = new aws.s3.BucketV2("afterstay-media", {
  bucket: "afterstay-media",
  forceDestroy: true,
});

new aws.s3.BucketPublicAccessBlock("afterstay-media-public-access-block", {
  bucket: mediaBucket.id,
  blockPublicAcls: true,
  blockPublicPolicy: true,
  ignorePublicAcls: true,
  restrictPublicBuckets: true,
});
