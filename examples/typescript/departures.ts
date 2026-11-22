// Lists the next departures at a stop on the HAFAS server of VBB, the
// transport association of Berlin and Brandenburg.

import { bundledNetwork, Client, delaySeconds } from "@kiliankoe/publictransport";

// Servers want to know who asks: name your application and a way to reach you.
const client = new Client(
  "publictransport-example (+https://github.com/kiliankoe/publictransport)",
);
const network = bundledNetwork("vbb-hafas");

const [stop] = await client.locations(network, "S+U Alexanderplatz", { kinds: ["stop"] });
if (!stop) {
  throw new Error("the stop was not found");
}

console.log(`Departures at ${stop.name}:`);
// From now on. `time` next to the limit asks for another time.
for (const departure of await client.departures(network, stop, { limit: 10 })) {
  // Times are `Date` values. With the time zone of the stop, where the
  // server names it, one reads as on the clocks there.
  const { timezone } = departure.stop;
  const time = timezone
    ? departure.scheduledTime.toLocaleTimeString("en-GB", {
        timeZone: timezone,
        timeStyle: "short",
      })
    : "??:??";
  // Only departures with real-time data have a delay.
  const seconds = delaySeconds(departure);
  const late = seconds === undefined ? undefined : Math.trunc(seconds / 60);
  const delay = late === undefined ? "" : `${late < 0 ? "" : "+"}${late}`;
  console.log(
    `  ${time} ${delay.padStart(3)} ${departure.line.name.padEnd(5)} ${departure.direction ?? "?"}${
      departure.cancelled ? " (cancelled)" : ""
    }`,
  );
  // What the operator reports about the ride, such as a closed road.
  for (const remark of departure.remarks ?? []) {
    if (remark.kind === "disruption" && remark.title) {
      console.log(`            ! ${remark.title}`);
    }
  }
}
