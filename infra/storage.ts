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

// Public-read APK downloads (fast alternative to GitHub Releases).
export const releasesBucket = new aws.s3.BucketV2("afterstay-releases", {
  bucket: "afterstay-releases",
  forceDestroy: true,
});

// Allow the bucket policy below to grant public read.
new aws.s3.BucketPublicAccessBlock("afterstay-releases-public-access-block", {
  bucket: releasesBucket.id,
  blockPublicAcls: true,
  blockPublicPolicy: false,
  ignorePublicAcls: true,
  restrictPublicBuckets: false,
});

new aws.s3.BucketPolicy("afterstay-releases-policy", {
  bucket: releasesBucket.id,
  policy: releasesBucket.arn.apply((arn) =>
    JSON.stringify({
      Version: "2012-10-17",
      Statement: [
        {
          Sid: "PublicReadGetObject",
          Effect: "Allow",
          Principal: "*",
          Action: "s3:GetObject",
          Resource: `${arn}/*`,
        },
      ],
    }),
  ),
});
