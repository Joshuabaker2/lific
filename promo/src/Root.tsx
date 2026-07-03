import React from "react";
import { Composition, Still } from "remotion";
import { Ad } from "./Ad";
import { Hero } from "./Hero";
import { TOTAL_FRAMES } from "./timing";
import { FPS, WIDTH, HEIGHT } from "./theme";
import "./index.css";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="Ad"
        component={Ad}
        durationInFrames={TOTAL_FRAMES}
        fps={FPS}
        width={WIDTH}
        height={HEIGHT}
      />
      {/* README hero image: npx remotion still Hero ../LificHero.png */}
      <Still id="Hero" component={Hero} width={1920} height={1080} />
    </>
  );
};
