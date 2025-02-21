const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

const dist = path.resolve(__dirname, "dist");

module.exports = {
    mode: "production",
    entry: {
        index: "./ts/src/index.ts",
    },
    output: {
        path: dist,
        filename: "[name].js",
    },
    module: {
        rules: [
            {
                test: /\.tsx?$/,
                use: [
                    {
                        loader: "ts-loader",
                        options: {
                            compilerOptions: {
                                module: "es2022",
                            },
                        },
                    },
                ],
                exclude: /node_modules/,
            },
        ],
    },
    devServer: {
        static: {
            directory: dist,
        },
    },
    plugins: [
        new CopyPlugin({
            patterns: [{ from: "static", to: "" }],
        }),

        new WasmPackPlugin({
            crateDirectory: __dirname,
        }),
    ],
    experiments: {
        asyncWebAssembly: true,
    },
    resolve: {
        extensions: [".tsx", ".ts", ".js"],
    },
};
