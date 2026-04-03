import { AXIS_DEFINITIONS } from "./contracts.js";
import { round } from "./util.js";

export const DEFAULT_OPENAI_MODEL = "gpt-4.1-mini";

function buildAxisSchema() {
  const properties = {};
  const required = [];

  for (const axis of AXIS_DEFINITIONS) {
    properties[axis.key] = {
      type: "number",
      minimum: 0,
      maximum: 100
    };
    required.push(axis.key);
  }

  return {
    type: "object",
    additionalProperties: false,
    properties,
    required
  };
}

function buildResponseSchema() {
  return {
    type: "object",
    additionalProperties: false,
    properties: {
      winner_id: {
        type: "string",
        enum: ["left", "right"]
      },
      left_scores: buildAxisSchema(),
      right_scores: buildAxisSchema(),
      sections: {
        type: "object",
        additionalProperties: false,
        properties: {
          overall_take: { type: "string" },
          strengths_left: { type: "string" },
          strengths_right: { type: "string" },
          weaknesses_left: { type: "string" },
          weaknesses_right: { type: "string" },
          why_this_won: { type: "string" },
          model_jury_notes: { type: "string" }
        },
        required: [
          "overall_take",
          "strengths_left",
          "strengths_right",
          "weaknesses_left",
          "weaknesses_right",
          "why_this_won",
          "model_jury_notes"
        ]
      }
    },
    required: ["winner_id", "left_scores", "right_scores", "sections"]
  };
}

function buildPrompt(leftPortrait, rightPortrait) {
  const axes = AXIS_DEFINITIONS.map(axis => `${axis.key}: ${axis.label}`).join("\n");

  return [
    "You are BetterThanYou, a visual battle judge for fictional AI-generated adult portraits.",
    "Do not treat the images as real people. Judge only the image result and presentation quality.",
    "Return one winner and score both portraits on every axis from 0 to 100.",
    "Keep the winner consistent with the score spread.",
    "Use concise, high-signal language.",
    `Left portrait label: ${leftPortrait.label}`,
    `Right portrait label: ${rightPortrait.label}`,
    "Axes:",
    axes
  ].join("\n");
}

function getOutputText(responsePayload) {
  if (typeof responsePayload.output_text === "string" && responsePayload.output_text.trim()) {
    return responsePayload.output_text;
  }

  const pieces = [];
  for (const item of responsePayload.output || []) {
    for (const content of item.content || []) {
      if (typeof content.text === "string") {
        pieces.push(content.text);
      }
      if (content.json) {
        pieces.push(JSON.stringify(content.json));
      }
    }
  }

  return pieces.join("\n").trim();
}

function normalizeAxisScores(scores) {
  const output = {};
  for (const axis of AXIS_DEFINITIONS) {
    const rawValue = Number(scores?.[axis.key] ?? 0);
    output[axis.key] = round(Math.min(100, Math.max(0, rawValue)));
  }
  return output;
}

export async function judgePortraitBattleWithOpenAI({
  leftPortrait,
  rightPortrait,
  model = DEFAULT_OPENAI_MODEL,
  apiKey = process.env.BTY_OPENAI_API_KEY || process.env.OPENAI_API_KEY,
  baseUrl = process.env.OPENAI_BASE_URL || "https://api.openai.com/v1",
  fetchImpl = fetch
}) {
  if (!apiKey) {
    throw new Error("OpenAI judging requires OPENAI_API_KEY or BTY_OPENAI_API_KEY.");
  }

  const response = await fetchImpl(`${baseUrl.replace(/\/$/, "")}/responses`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${apiKey}`
    },
    body: JSON.stringify({
      model,
      input: [
        {
          role: "user",
          content: [
            {
              type: "input_text",
              text: buildPrompt(leftPortrait, rightPortrait)
            },
            {
              type: "input_image",
              image_url: leftPortrait.imageDataUrl,
              detail: "high"
            },
            {
              type: "input_image",
              image_url: rightPortrait.imageDataUrl,
              detail: "high"
            }
          ]
        }
      ],
      text: {
        format: {
          type: "json_schema",
          name: "better_than_you_battle",
          strict: true,
          schema: buildResponseSchema()
        }
      }
    })
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`OpenAI judge failed: HTTP ${response.status} ${errorText}`);
  }

  const payload = await response.json();
  const outputText = getOutputText(payload);
  if (!outputText) {
    throw new Error("OpenAI judge returned no output text.");
  }

  const parsed = JSON.parse(outputText);
  return {
    winnerId: parsed.winner_id,
    leftScores: normalizeAxisScores(parsed.left_scores),
    rightScores: normalizeAxisScores(parsed.right_scores),
    sections: {
      overallTake: parsed.sections.overall_take,
      strengths: {
        left: parsed.sections.strengths_left,
        right: parsed.sections.strengths_right
      },
      weaknesses: {
        left: parsed.sections.weaknesses_left,
        right: parsed.sections.weaknesses_right
      },
      whyThisWon: parsed.sections.why_this_won,
      modelJuryNotes: parsed.sections.model_jury_notes
    },
    provider: "openai",
    model,
    raw: parsed
  };
}
