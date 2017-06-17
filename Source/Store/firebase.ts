import {User} from "./firebase/users";
import UserExtraInfo from "./firebase/userExtras/@UserExtraInfo";
import {MapNode} from "./firebase/nodes/@MapNode";
import {RatingsSet} from "./firebase/nodeRatings/@RatingsRoot";
import {Term} from "./firebase/terms/@Term";
import {Map} from "./firebase/maps/@Map";
import TermComponent from "./firebase/termComponents/@TermComponent";
import { GeneralData } from "./firebase/general";
import {ViewerSet} from "./firebase/nodeViewers/@ViewerSet";
import {Image} from "./firebase/images/@Image";
import {MapNodeStats} from "Store/firebase/nodeStats/@MapNodeStats";
import {ViewedNodeSet} from "./firebase/userViewedNodes/@ViewedNodeSet";

export interface FirebaseData {
	general: GeneralData;
	images: {[key: string]: Image};
	maps: {[key: number]: Map};
	nodes: {[key: number]: MapNode};
	nodeExtras: {[key: number]: any};
	nodeRatings: {[key: number]: RatingsSet};
	nodeStats: {[key: number]: MapNodeStats};
	nodeViewers: {[key: number]: ViewerSet};
	terms: {[key: number]: Term};
	termComponents: {[key: number]: TermComponent};
	termNames: {[key: string]: any};
	users: {[key: string]: User};
	userExtras: {[key: string]: UserExtraInfo};
	userViewedNodes: {[key: string]: ViewedNodeSet};
}