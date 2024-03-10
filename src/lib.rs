mod utils;

use core::time;
use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, RwLock};
use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, MessageEvent, WebSocket, Request, RequestInit, Response, window};
use serde_json::Value;
use wasm_bindgen_futures::JsFuture;

extern crate js_sys;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}
#[derive(Copy, Clone)]
pub struct Trade {
    price: f64,
    quantity: f64,
    time: u64,
    is_buyer_maker: bool,
}
#[derive(Clone, Debug)]
pub struct Order {
    price: f64,
    quantity: f64,
}
#[derive(Debug)]
pub struct Kline {
    open_time: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    buy_volume: f64,
    sell_volume: f64,
    close_time: u64,
}
#[derive(Debug)]
pub struct TradeGroups {
    buy: HashMap<i64, f64>,
    sell: HashMap<i64, f64>,
}

#[wasm_bindgen]
pub struct CanvasManager {
    klines_ohlcv: Arc<RwLock<BTreeMap<u64, Kline>>>,
    klines_trades: Arc<RwLock<BTreeMap<u64, TradeGroups>>>,
    orderbook_manager: OrderbookManager,
    canvas_main: CanvasMain,
    canvas_orderbook: CanvasOrderbook,
    canvas_indicator_volume: CanvasIndicatorVolume,
    canvas_bubble: Rc<RefCell<CanvasBubbleTrades>>,
    pan_x_offset: f64,
    x_zoom: f64,
    bucket_size: Arc<RwLock<f64>>
}
#[wasm_bindgen]
impl CanvasManager {
    pub fn new(canvas1: HtmlCanvasElement, canvas2: HtmlCanvasElement, canvas3: HtmlCanvasElement, canvas4: HtmlCanvasElement) -> Self {
        Self {
            klines_ohlcv: Arc::new(RwLock::new(BTreeMap::new())),
            klines_trades: Arc::new(RwLock::new(BTreeMap::new())),
            orderbook_manager: OrderbookManager::new(),
            canvas_main: CanvasMain::new(canvas1).expect("Failed to create CanvasMain"),
            canvas_orderbook: CanvasOrderbook::new(canvas2).expect("Failed to create CanvasOrderbook"),
            canvas_indicator_volume: CanvasIndicatorVolume::new(canvas3).expect("Failed to create CanvasIndicatorVolume"),
            canvas_bubble: Rc::new(RefCell::new(CanvasBubbleTrades::new(canvas4).expect("Failed to create CanvasBubbleTrades"))),
            pan_x_offset: 0.0,
            x_zoom: 30.0,
            bucket_size: Arc::new(RwLock::new(0.5)),
        }
    }
    
    pub async fn initialize_ws(&mut self) {  
        self.start_websocket();
        
        let mut klines_ohlcv = self.klines_ohlcv.try_write().unwrap();
        if (klines_ohlcv.len() as f64) < 60.0 {
            let limit = 60.0 - klines_ohlcv.len() as f64;
            match get_hist_klines("btcusdt", "1m", limit).await {
                Ok(klines) => {
                    let klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&klines.as_string().unwrap()).unwrap();
                    for kline in klines {
                        let open_time = kline[0].as_u64().unwrap();
                        let open = kline[1].as_str().unwrap().parse::<f64>().unwrap();
                        let high = kline[2].as_str().unwrap().parse::<f64>().unwrap();
                        let low = kline[3].as_str().unwrap().parse::<f64>().unwrap();
                        let close = kline[4].as_str().unwrap().parse::<f64>().unwrap();
                        let volume = kline[5].as_str().unwrap().parse::<f64>().unwrap();
                        let close_time = kline[6].as_u64().unwrap();
                        let buy_volume = kline[9].as_str().unwrap().parse::<f64>().unwrap();
                        let sell_volume = volume - buy_volume;
                        let kline = Kline {
                            open_time, 
                            open, high, low, close, 
                            buy_volume, sell_volume, 
                            close_time,
                        };
                        klines_ohlcv.insert(open_time, kline);
                    }
                },
                Err(e) => {
                    log(&format!("Failed to fetch klines: {:?}", e));
                }
            }
        }
    }
   
    pub fn start_websocket(&mut self) {
        let ws = WebSocket::new("wss://fstream.binance.com/stream?streams=btcusdt@aggTrade/btcusdt@depth@100ms/btcusdt@kline_1m").unwrap();

        let mut current_kline_open: u64 = 0;
        let mut trades_buffer: Vec<Trade> = Vec::new();
        let bucket_size = Arc::clone(&self.bucket_size); 

        let klines_trades = Arc::clone(&self.klines_trades);
        let klines_ohlcv = Arc::clone(&self.klines_ohlcv);

        let bids = Arc::clone(&self.orderbook_manager.bids);
        let asks = Arc::clone(&self.orderbook_manager.asks);
        //let last_update_id = Arc::clone(&self.orderbook_manager.last_update_id);

        let canvas_bubble = Rc::clone(&self.canvas_bubble);

        let onmessage_callback = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Ok(data) = event.data().dyn_into::<js_sys::JsString>() {
                let data_str: String = data.into();
                let v: Value = serde_json::from_str(&data_str).unwrap();

                let event = match web_sys::CustomEvent::new("renderEvent") {
                    Ok(event) => event,
                    Err(error) => {
                        log(&format!("Failed to create custom event: {:?}", error));
                        return;
                    }
                };

                match v["stream"].as_str() {
                    Some(stream) if stream.contains("aggTrade") => {
                        if let (Some(price), Some(quantity), Some(time), Some(is_buyer_maker)) = 
                        (v["data"]["p"].as_str(), v["data"]["q"].as_str(), v["data"]["T"].as_u64(), v["data"]["m"].as_bool()) {
                            if let (Ok(price), Ok(quantity)) = (price.parse::<f64>(), quantity.parse::<f64>()) {
                                let trade = Trade { price, quantity, time, is_buyer_maker };
                                trades_buffer.push(trade);
                            }
                        }
                    },
                    Some(stream) if stream.contains("depth") => {
                        if let Some(bids_array) = v["data"]["b"].as_array() {
                            if let Ok(mut bids_borrowed) = bids.try_write() {
                                let min_price = bids_borrowed.iter().map(|x| x.price).fold(f64::INFINITY, |a, b| a.min(b));
                                let bids_array: Vec<&Value> = bids_array.iter().filter(|x| {
                                    if let Some(price) = x[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        price >= min_price
                                    } else {
                                        false
                                    }
                                }).collect();
                                for bid in bids_array {
                                    let price = match bid[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(price) => price,
                                        None => continue,
                                    };
                                    let quantity = match bid[1].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(quantity) => quantity,
                                        None => continue,
                                    };
                                    if quantity == 0.0 {
                                        bids_borrowed.retain(|x| x.price != price);
                                    } else if let Some(bid) = bids_borrowed.iter_mut().find(|x| x.price == price) {
                                        bid.quantity = quantity;
                                    } else {
                                        bids_borrowed.push(Order { price, quantity });
                                    }
                                }
                            } else {
                                log(&format!("bids locked on render"));
                            }
                        }
                        if let Some(asks_array) = v["data"]["a"].as_array() {
                            if let Ok(mut asks_borrowed) = asks.try_write() {
                                let max_price = asks_borrowed.iter().map(|x| x.price).fold(f64::NEG_INFINITY, |a, b| a.max(b));
                                let asks_array: Vec<&Value> = asks_array.iter().filter(|x| {
                                    if let Some(price) = x[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        price <= max_price
                                    } else {
                                        false
                                    }
                                }).collect();
                                for ask in asks_array {
                                    let price = match ask[0].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(price) => price,
                                        None => continue,
                                    };
                                    let quantity = match ask[1].as_str().and_then(|s| s.parse::<f64>().ok()) {
                                        Some(quantity) => quantity,
                                        None => continue,
                                    };
                                    if quantity == 0.0 {
                                        asks_borrowed.retain(|x| x.price != price);
                                    } else if let Some(ask) = asks_borrowed.iter_mut().find(|x| x.price == price) {
                                        ask.quantity = quantity;
                                    } else {
                                        asks_borrowed.push(Order { price, quantity });
                                    }
                                }
                            } else {
                                log(&format!("asks locked on render"));
                            }
                        }
    
                        if current_kline_open != 0 {
                            match klines_trades.try_write() {
                                Ok(mut klines_trades) => {
                                    let bucket_size_lock = bucket_size.read().unwrap();
                                    let trade_groups = klines_trades.entry(current_kline_open).or_insert(TradeGroups { buy: HashMap::new(), sell: HashMap::new() });
                                    for trade in &trades_buffer {
                                        let price_as_int = ((trade.price / *bucket_size_lock).round() * *bucket_size_lock * 100.0) as i64;
                                        let quantity_sum = if trade.is_buyer_maker {
                                            trade_groups.sell.entry(price_as_int).or_insert(0.0)
                                        } else {
                                            trade_groups.buy.entry(price_as_int).or_insert(0.0)
                                        };
                                        *quantity_sum += trade.quantity;
                                    }

                                    if let Some(update_time) = v["data"]["T"].as_u64() {
                                        let mut canvas_bubble = canvas_bubble.borrow_mut();
                                        canvas_bubble.render(&trades_buffer, update_time);
                                    }
                                    trades_buffer.clear();
                                },
                                Err(poisoned) => {
                                    log(&format!("klines_trades locked on render: {:?}", poisoned));
                                }
                            }
                        }
                        match web_sys::window() {
                            Some(window) => {
                                if let Err(error) = window.dispatch_event(&event) {
                                    log(&format!("Failed to dispatch event: {:?}", error));
                                }
                            },
                            None => log("No window available"),
                        }
                    },
                    Some(stream) if stream.contains("kline") => {
                        if let Some(kline_data) = v["data"]["k"].as_object() {
                            let open_time = kline_data["t"].as_u64();
                            let open = kline_data["o"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let high = kline_data["h"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let low = kline_data["l"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let close = kline_data["c"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let volume = kline_data["v"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let buy_volume = kline_data["V"].as_str().and_then(|s| s.parse::<f64>().ok());
                            let sell_volume = match (volume, buy_volume) {
                                (Some(volume), Some(buy_volume)) => Some(volume - buy_volume),
                                _ => None,
                            };
                            let close_time = kline_data["T"].as_u64();
                    
                            if let (
                                Some(open_time), 
                                Some(open), Some(high), Some(low), Some(close), 
                                Some(buy_volume), Some(sell_volume), 
                                Some(close_time)) = (open_time, open, high, low, close, buy_volume, sell_volume, close_time) {
                                let kline = Kline {
                                    open_time, 
                                    open, high, low, close,
                                    buy_volume, sell_volume,
                                    close_time,
                                };
                                if let Ok(mut klines_ohlcv) = klines_ohlcv.try_write() {
                                    klines_ohlcv.insert(open_time, kline);  
                                }
                                current_kline_open = open_time;
                            }
                        }
                    },
                    _ => {
                       log(&format!("Unknown stream: {:?}", v));
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
    }   

    pub fn render(&mut self) {
        match self.klines_ohlcv.try_read() {
            Ok(klines_borrowed) => {
                let zoom_scale = self.x_zoom * 60.0 * 1000.0;
                let last_time = match klines_borrowed.iter().last() {
                    Some((last_time, _)) => *last_time,
                    None => return, 
                };
                let time_difference = last_time + 60000 - zoom_scale as u64;

                let left_x = 0 - self.pan_x_offset as i64;
                let right_x = self.canvas_main.width as i64 - self.pan_x_offset as i64;

                let visible_klines: Vec<_> = klines_borrowed.iter().filter(|&(open_time, _)| {
                    let x = ((open_time - time_difference) as f64 / zoom_scale) * self.canvas_main.width;
                    x >= left_x as f64 && x <= right_x as f64
                }).collect();

                let avg_body_length: f64 = visible_klines.iter()
                    .map(|(_, kline)| (kline.close - kline.open).abs())
                    .sum::<f64>() / visible_klines.len() as f64;

                let y_max = visible_klines.iter().map(|(_, kline)| kline.high).fold(0.0, f64::max) + avg_body_length;
                let y_min = visible_klines.iter().map(|(_, kline)| kline.low).fold(f64::MAX, f64::min) - avg_body_length;

                self.canvas_indicator_volume.render(&visible_klines);

                match (self.orderbook_manager.bids.try_read(), self.orderbook_manager.asks.try_read()) {
                    (Ok(bids_borrowed), Ok(asks_borrowed)) => {
                        let bucket_size = self.bucket_size.read().unwrap();

                        let grouped_bids = group_orders(*bucket_size, &bids_borrowed);
                        let grouped_asks = group_orders(*bucket_size, &asks_borrowed);

                        self.canvas_orderbook.render(y_min, y_max, &grouped_bids, &grouped_asks, &visible_klines);

                        match self.klines_trades.try_read() {
                            Ok(klines_trades_borrowed) => {    
                                let visible_trades: Vec<_> = klines_trades_borrowed.iter().filter(|&(open_time, _)| {
                                    let x = ((open_time - time_difference) as f64 / zoom_scale) * self.canvas_main.width;
                                    x >= left_x as f64 && x <= right_x as f64
                                }).collect();  
                                    
                                self.canvas_main.render(y_min, y_max, &visible_klines, &visible_trades);
                            },
                            Err(e) => {
                                log(&format!("Failed to acquire lock on klines_trades during render: {}", e));
                            }
                        }
                    },
                    (Err(e), _) => {
                        log(&format!("Failed to acquire lock on bids during render: {}", e));
                    },
                    (_, Err(e)) => {
                        log(&format!("Failed to acquire lock on asks during render: {}", e));
                    }
                }
            },
            Err(e) => {
                log(&format!("Failed to acquire lock on klines during render: {}", e));
            }
        }
    }

    pub fn fetch_depth(&mut self, depth: JsValue) {
        self.orderbook_manager.fetch_depth(depth);
    }
}

pub struct CanvasOrderbook {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
}
impl CanvasOrderbook {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }

    pub fn render(&mut self, y_min: f64, y_max: f64, bids: &Vec<Order>, asks: &Vec<Order>, klines: &Vec<(&u64, &Kline)>) {
        let context = &self.ctx;
        self.ctx.clear_rect(0.0, 0.0, self.width, self.height);

        let max_quantity = {
            let max_bid_quantity = bids.iter().fold(0.0, |max, x| x.quantity.max(max));
            let max_ask_quantity = asks.iter().fold(0.0, |max, x| x.quantity.max(max));
            max_bid_quantity.max(max_ask_quantity)
        }; 

        let num_labels = 12; 
        let step = (y_max - y_min) / num_labels as f64;
        context.set_font("20px monospace");
        context.set_fill_style(&"rgba(200, 200, 200, 0.8)".into());

        for i in 0..=num_labels {
            let y_value = y_min + step * i as f64;
            let y = self.height - ((y_value - y_min) / (y_max - y_min)) * self.height as f64;

            let y_value_str = format!("{:.1}", y_value);

            context.fill_text(&y_value_str, 6.0, y).unwrap();
        }

        if let Some(_best_bid) = bids.first() {     
            context.set_stroke_style(&"rgba(81, 205, 160, 1)".into());
            for (_i, bid) in bids.iter().enumerate() {
                let x = (bid.quantity / (max_quantity + max_quantity/4.0)) * (self.width - 120.0) as f64;
                let y = ((bid.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.begin_path();
                context.move_to(108.0, self.height - y);
                context.line_to(108.0 + x, self.height - y);
                context.stroke();
            }
        }
        if let Some(_best_ask) = asks.first() {
            context.set_stroke_style(&"rgba(192, 80, 77, 1)".into());
            for (_i, ask) in asks.iter().enumerate() {
                let x = (ask.quantity / (max_quantity + max_quantity/4.0)) * (self.width - 120.0) as f64;
                let y = ((ask.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.begin_path();
                context.move_to(108.0, self.height - y);
                context.line_to(108.0 + x, self.height - y);
                context.stroke();
            }

        }
        klines.last().map(|(_last_time, kline)| {
            let y = ((kline.close - y_min) / (y_max - y_min)) * self.height as f64;
            let y_value_str = format!("{:.1}", kline.close);

            if kline.open < kline.close {
                context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
            } else {
                context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
            }
            let rect_y = self.height - y - 20.0; 
            context.fill_rect(6.0, rect_y, 90.0, 30.0);
            
            context.set_fill_style(&"black".into());
            context.fill_text(&y_value_str, 6.0, self.height - y).unwrap();
        });
        let max_quantity_str = format!("{:.1}", max_quantity);
        context.set_fill_style(&"rgba(200, 200, 200, 0.8)".into());
        context.set_font("18px monospace");
        context.fill_text(&max_quantity_str, self.width - 100.0, 20.0).unwrap();
    } 
}
pub struct CanvasMain {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    x_zoom: f64,
}
impl CanvasMain {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    x_zoom: 30.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn render(&mut self, y_min: f64, y_max: f64, klines: &Vec<(&u64, &Kline)>, trades: &Vec<(&u64, &TradeGroups)>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);

        let zoom_scale = self.x_zoom * 60.0 * 1000.0;

        if let Some((last_time, _)) = klines.iter().last() {
            let time_difference = *last_time + 60000 - zoom_scale as u64;
            let rect_width = (self.width as f64 / (&self.x_zoom/2.0)) / 5.0;
            
            let max_quantity = trades.iter().flat_map(|(_, trade_groups)| {
                trade_groups.buy.iter().chain(trade_groups.sell.iter()).map(|(_, quantity)| *quantity)
            }).fold(0.0, f64::max);
    
            context.set_line_width(1.0);
            for (_i, (_, kline)) in klines.iter().enumerate() {
                let x = ((kline.open_time - time_difference) as f64 / zoom_scale) * self.width;

                let y_open = self.height as f64 * (kline.open - y_min) / (y_max - y_min);
                let y_close = self.height as f64 * (kline.close - y_min) / (y_max - y_min);
                let y_high = self.height as f64 * (kline.high - y_min) / (y_max - y_min);
                let y_low = self.height as f64 * (kline.low - y_min) / (y_max - y_min);

                context.set_stroke_style(&(if kline.open < kline.close { "green" } else { "red" }).into());
                context.begin_path();
                context.move_to(x + rect_width, self.height - y_open);
                context.line_to(x + rect_width, self.height - y_close);
                context.stroke();

                context.set_stroke_style(&"rgba(200, 200, 200, 0.5)".into());
                context.begin_path();
                context.move_to(x, self.height - y_high);
                context.line_to(x + (rect_width*2.0), self.height - y_high);

                context.move_to(x, self.height - y_low);
                context.line_to(x + (rect_width*2.0), self.height - y_low);
                context.stroke();

                if let Some((_, trade_groups)) = trades.iter().find(|&&(time, _)| *time == kline.open_time) {
                    context.set_stroke_style(&"rgba(81, 205, 160, 1)".into());
                    for (price_as_int, quantity) in &trade_groups.buy { 
                        let price = *price_as_int as f64 / 100.0;
                        let y_trade = self.height as f64 * (price - y_min) / (y_max - y_min);
                        let scaled_quantity = rect_width as f64 * quantity / max_quantity;

                        context.begin_path();
                        context.move_to(x + rect_width + 4.0, self.height - y_trade);
                        context.line_to(x + rect_width + 4.0 + scaled_quantity, self.height - y_trade);
                        context.stroke();
                    }
                    context.set_stroke_style(&"rgba(192, 80, 77, 1)".into());
                    for (price_as_int, quantity) in &trade_groups.sell {
                        let price = *price_as_int as f64 / 100.0;
                        let y_trade = self.height as f64 * (price - y_min) / (y_max - y_min);
                        let scaled_quantity = rect_width as f64 * quantity / max_quantity;

                        context.begin_path();
                        context.move_to(x + rect_width - 4.0, self.height - y_trade);
                        context.line_to(x + rect_width - 4.0 - scaled_quantity, self.height - y_trade);
                        context.stroke();
                    }
                }
            }
        }
    }
}
pub struct CanvasIndicatorVolume {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    x_zoom: f64,
}
impl CanvasIndicatorVolume {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    x_zoom: 30.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn render(&mut self, klines: &Vec<(&u64, &Kline)>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        
        let zoom_scale = self.x_zoom * 60.0 * 1000.0;

        match klines.iter().last() {
            Some((last_time, _)) => {
                let max_volume = klines.iter().map(|(_, kline)| f64::max(kline.buy_volume, kline.sell_volume)).fold(0.0, f64::max);
                let time_difference = *last_time + 60000 - zoom_scale as u64;
                let rect_width = (self.width as f64 / (&self.x_zoom/2.0)) / 5.0;
      
                for (_i, (_, kline)) in klines.iter().enumerate() {
                    let x = ((kline.open_time - time_difference) as f64 / zoom_scale) * self.width;
                
                    let buy_height = self.height as f64 * (kline.buy_volume / max_volume);
                    let sell_height = self.height as f64 * (kline.sell_volume / max_volume);
                
                    context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
                    context.fill_rect(x + rect_width, self.height as f64 - buy_height, rect_width, buy_height);
                
                    context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
                    context.fill_rect(x, self.height as f64 - sell_height, rect_width, sell_height);
                }
            },
            None => {
                log(&format!("No klines"));
            }
        }
    }
}
pub struct CanvasBubbleTrades {
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    trades: Vec<Trade>,
    sell_trade_counts: BTreeMap<u64, usize>,
    buy_trade_counts: BTreeMap<u64, usize>,
}
impl CanvasBubbleTrades {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    ctx,
                    width,
                    height,
                    trades: Vec::new(),
                    sell_trade_counts: BTreeMap::new(),
                    buy_trade_counts: BTreeMap::new(),
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn render(&mut self, trades_buffer: &Vec<Trade>, last_update: u64) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);

        let padding_percentage = 0.25; 
        let padded_height = self.height * (1.0 - padding_percentage);
        let thirty_seconds_ago = last_update - 30 * 1000;

        // Trade counts
        self.buy_trade_counts.retain(|&time, _| time >= thirty_seconds_ago);
        self.sell_trade_counts.retain(|&time, _| time >= thirty_seconds_ago);

        let sell_count = trades_buffer.iter()
            .filter(|trade| trade.is_buyer_maker)
            .count();
        self.sell_trade_counts.insert(last_update, sell_count);
        let buy_count = trades_buffer.iter()
            .filter(|trade| !trade.is_buyer_maker)
            .count();
        self.buy_trade_counts.insert(last_update, buy_count);

        let max_trade_count = self.sell_trade_counts.iter().chain(self.buy_trade_counts.iter())
            .map(|(_, &count)| count)
            .fold(0, usize::max);
        let y_scale = padded_height / max_trade_count as f64;

        context.set_stroke_style(&"rgba(200, 50, 50, 0.4)".into());
        let mut previous_point: Option<(f64, f64)> = None;
        for (&time, &count) in self.sell_trade_counts.iter() {
            let x = ((time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
            let y = self.height / 2.0 + count as f64 * y_scale;
            match previous_point {
                Some((prev_x, prev_y)) => {
                    context.begin_path();
                    context.move_to(prev_x, prev_y);
                    context.line_to(x, y);
                    context.stroke();
                },
                None => (),
            }
            previous_point = Some((x, y));
        }
        context.set_stroke_style(&"rgba(50, 200, 50, 0.4)".into());
        previous_point = None;
        for (&time, &count) in self.buy_trade_counts.iter() {
            let x = ((time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
            let y = self.height / 2.0 - count as f64 * y_scale;
            match previous_point {
                Some((prev_x, prev_y)) => {
                    context.begin_path();
                    context.move_to(prev_x, prev_y);
                    context.line_to(x, y);
                    context.stroke();
                },
                None => (),
            }
            previous_point = Some((x, y));
        }

        // Bubble trades
        self.trades.retain(|trade| trade.time >= thirty_seconds_ago);

        for trade in trades_buffer {
            self.trades.push(*trade);
        }
        
        let max_quantity = self.trades.iter().map(|trade| trade.quantity).fold(0.0, f64::max);
        let y_min = self.trades.iter().map(|trade| trade.price).fold(f64::MAX, f64::min);
        let y_max = self.trades.iter().map(|trade| trade.price).fold(0.0, f64::max);

        let sell_trades: Vec<_> = self.trades.iter().filter(|trade| trade.is_buyer_maker).collect();
        let buy_trades: Vec<_> = self.trades.iter().filter(|trade| !trade.is_buyer_maker).collect();

        context.set_fill_style(&"rgba(192, 80, 77, 1)".into());
        for trade in &sell_trades {
            let radius = (trade.quantity / max_quantity) * 40.0;       
            if radius > 1.0 {
                let x = ((trade.time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
                let y = ((trade.price - y_min) / (y_max - y_min)) * padded_height + self.height * padding_percentage / 2.0;

                context.begin_path();
                context.arc(x, self.height - y, radius, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                context.fill();
            }
        }
        context.set_fill_style(&"rgba(81, 205, 160, 1)".into());
        for trade in &buy_trades {
            let radius = (trade.quantity / max_quantity) * 40.0;
            if radius > 1.0 {
                let x = ((trade.time - thirty_seconds_ago) as f64 / 30000.0) * self.width;
                let y = ((trade.price - y_min) / (y_max - y_min)) * padded_height + self.height * padding_percentage / 2.0;

                context.begin_path();
                context.arc(x, self.height - y, radius, 0.0, 2.0 * std::f64::consts::PI).unwrap();
                context.fill();
            }
        }
    }
}

pub async fn get_hist_klines(symbol: &str, interval: &str, limit: f64) -> Result<JsValue, JsValue> {
    let window = window().expect("no global `window` exists");
    let url = format!("https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit={}", symbol, interval, limit); 
    let request = Request::new_with_str_and_init(&url, &RequestInit::new())?;

    let response = JsFuture::from(window.fetch_with_request(&request)).await?;
    let response: Response = response.dyn_into()?;

    let text = JsFuture::from(response.text()?).await?;
    Ok(text)
}
pub fn group_orders(bucket_size: f64, orders: &Vec<Order>) -> Vec<Order> {
    let mut grouped_orders = HashMap::new();
    for order in orders {
        let price = ((order.price / bucket_size).round() * bucket_size * 100.0) as i64;
        let quantity = grouped_orders.entry(price).or_insert(0.0);
        *quantity += order.quantity;
    }
    let mut orders = Vec::new();
    for (price, quantity) in grouped_orders {
        let price = price as f64 / 100.0; 
        orders.push(Order { price, quantity });
    }
    orders
}
pub struct OrderbookManager {
    bids: Arc<RwLock<Vec<Order>>>,
    asks: Arc<RwLock<Vec<Order>>>,
    last_update_id: Arc<RwLock<u64>>,
}
impl OrderbookManager {
    pub fn new() -> Self {
        Self {
            bids: Arc::new(RwLock::new(Vec::new())),
            asks: Arc::new(RwLock::new(Vec::new())),
            last_update_id: Arc::new(RwLock::new(0)),
        }
    }
    pub fn fetch_depth(&mut self, depth: JsValue) {
        let depth: serde_json::Value = serde_json::from_str(&depth.as_string().unwrap()).unwrap();
        if let Some(bids_array) = depth["bids"].as_array() {
            if let Ok(mut bids_borrowed) = self.bids.try_write() {
                *bids_borrowed = bids_array.iter().filter_map(|x| {
                    x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                        .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                        .map(|quantity| Some(Order { price, quantity })).flatten())
                }).collect();
            } else {
                log(&format!("bids locked on render"));
            }
        }     
        if let Some(asks_array) = depth["asks"].as_array() {
            if let Ok(mut asks_borrowed) = self.asks.try_write() {
                *asks_borrowed = asks_array.iter().filter_map(|x| {
                    x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                        .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                        .map(|quantity| Some(Order { price, quantity })).flatten())
                }).collect();
            } else {
                log(&format!("asks locked on render"));
            }
        }
        let mut last_update_id = self.last_update_id.write().unwrap();
        *last_update_id = depth["lastUpdateId"].as_u64().unwrap();
    }
}